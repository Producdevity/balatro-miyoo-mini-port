use anyhow::Result;
use sprite_to_text::pixel_buffer::PixelBuffer;

#[cfg(any(target_os = "linux", test))]
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct BitField {
    offset: u32,
    length: u32,
    msb_right: u32,
}

#[cfg(any(target_os = "linux", test))]
#[derive(Clone, Copy)]
struct PixelFormat {
    red: BitField,
    green: BitField,
    blue: BitField,
    alpha: BitField,
}

#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    use anyhow::{bail, Context};
    use std::fs::{File, OpenOptions};
    use std::os::fd::AsRawFd;
    use std::ptr;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{Arc, Condvar, LazyLock, Mutex};
    use std::time::Instant;

    const FBIOGET_VSCREENINFO: u32 = 0x4600;
    const FBIOGET_FSCREENINFO: u32 = 0x4602;
    const FBIOPAN_DISPLAY: u32 = 0x4606;
    const FBIO_WAITFORVSYNC: u32 = 0x4004_4620;
    const FB_ACTIVATE_NOW: u32 = 0;

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct VarInfo {
        xres: u32,
        yres: u32,
        xres_virtual: u32,
        yres_virtual: u32,
        xoffset: u32,
        yoffset: u32,
        bits_per_pixel: u32,
        grayscale: u32,
        red: BitField,
        green: BitField,
        blue: BitField,
        transp: BitField,
        nonstd: u32,
        activate: u32,
        height: u32,
        width: u32,
        accel_flags: u32,
        pixclock: u32,
        left_margin: u32,
        right_margin: u32,
        upper_margin: u32,
        lower_margin: u32,
        hsync_len: u32,
        vsync_len: u32,
        sync: u32,
        vmode: u32,
        rotate: u32,
        colorspace: u32,
        reserved: [u32; 4],
    }

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct FixInfo {
        id: [libc::c_char; 16],
        smem_start: libc::c_ulong,
        smem_len: u32,
        type_: u32,
        type_aux: u32,
        visual: u32,
        xpanstep: u16,
        ypanstep: u16,
        ywrapstep: u16,
        line_length: u32,
        mmio_start: libc::c_ulong,
        mmio_len: u32,
        accel: u32,
        capabilities: u16,
        reserved: [u16; 2],
    }

    struct Framebuffer {
        file: File,
        mapping: *mut u8,
        mapping_len: usize,
        staging: Vec<u8>,
        source_shadows: Vec<Vec<u8>>,
        page_valid: Vec<bool>,
        present_count: u64,
        var: VarInfo,
        stride: usize,
        pages: usize,
        front_page: usize,
        wait_for_vsync: bool,
        rotate_180: bool,
        direct_present: bool,
        format: PixelFormat,
    }

    struct FrameSubmission {
        width: u32,
        height: u32,
        pixels: Vec<u8>,
    }

    struct PresenterQueue {
        pending: Mutex<Option<FrameSubmission>>,
        recycled: Mutex<Vec<Vec<u8>>>,
        ready: Condvar,
        error: Mutex<Option<String>>,
        submitted: AtomicU64,
        displayed: AtomicU64,
    }

    struct AsyncPresenter {
        queue: Arc<PresenterQueue>,
    }

    // The mapping belongs to the File and is only used while guarded by DEVICE.
    unsafe impl Send for Framebuffer {}

    impl Framebuffer {
        fn open() -> Result<Self> {
            let path =
                std::env::var("BALATRO_FRAMEBUFFER").unwrap_or_else(|_| "/dev/fb0".to_owned());
            let file = OpenOptions::new()
                .read(true)
                .write(true)
                .open(&path)
                .with_context(|| format!("open {path}"))?;
            let fd = file.as_raw_fd();
            let mut var = VarInfo::default();
            let mut fix = FixInfo::default();
            if unsafe { libc::ioctl(fd, FBIOGET_VSCREENINFO as _, &mut var) } != 0
                || unsafe { libc::ioctl(fd, FBIOGET_FSCREENINFO as _, &mut fix) } != 0
            {
                return Err(std::io::Error::last_os_error()).context("query framebuffer");
            }
            if var.bits_per_pixel != 32 || fix.line_length < var.xres * 4 {
                bail!(
                    "unsupported framebuffer: {}x{} {}bpp stride {}",
                    var.xres,
                    var.yres,
                    var.bits_per_pixel,
                    fix.line_length
                );
            }
            let mapping_len = fix.smem_len as usize;
            let mapping = unsafe {
                libc::mmap(
                    ptr::null_mut(),
                    mapping_len,
                    libc::PROT_READ | libc::PROT_WRITE,
                    libc::MAP_SHARED,
                    fd,
                    0,
                )
            };
            if mapping == libc::MAP_FAILED {
                return Err(std::io::Error::last_os_error()).context("map framebuffer");
            }
            let page_bytes = fix.line_length as usize * var.yres as usize;
            let virtual_pages = (var.yres_virtual / var.yres.max(1)) as usize;
            let memory_pages = mapping_len / page_bytes.max(1);
            let pages = virtual_pages.min(memory_pages).max(1);
            let front_page = ((var.yoffset / var.yres.max(1)) as usize).min(pages - 1);
            let format = PixelFormat {
                red: var.red,
                green: var.green,
                blue: var.blue,
                alpha: var.transp,
            };
            let rotate_180 = std::env::var("BALATRO_ROTATE_180").as_deref() == Ok("1");
            eprintln!(
                "[video] {}x{} stride={} pages={} rotate180={} rgba={}/{},{}/{},{}/{},{}/{}",
                var.xres,
                var.yres,
                fix.line_length,
                pages,
                rotate_180,
                var.red.offset,
                var.red.length,
                var.green.offset,
                var.green.length,
                var.blue.offset,
                var.blue.length,
                var.transp.offset,
                var.transp.length
            );
            Ok(Self {
                file,
                mapping: mapping.cast(),
                mapping_len,
                staging: vec![0; page_bytes],
                source_shadows: vec![Vec::new(); pages],
                page_valid: vec![false; pages],
                present_count: 0,
                var,
                stride: fix.line_length as usize,
                pages,
                front_page,
                wait_for_vsync: pages == 1,
                rotate_180,
                direct_present: std::env::var("BALATRO_DIRECT_PRESENT").as_deref() == Ok("1"),
                format,
            })
        }

        fn present(&mut self, frame: &FrameSubmission) -> Result<()> {
            if self.pages == 1 && self.wait_for_vsync {
                let argument: u32 = 0;
                if unsafe {
                    libc::ioctl(
                        self.file.as_raw_fd(),
                        FBIO_WAITFORVSYNC as _,
                        &argument as *const u32,
                    )
                } != 0
                {
                    self.wait_for_vsync = false;
                    eprintln!("[video] vsync ioctl unavailable; using immediate updates");
                }
            }

            let target_page = if self.pages > 1 {
                (self.front_page + 1) % self.pages
            } else {
                self.front_page
            };
            self.blit_to_page(frame, target_page);

            if self.pages > 1 {
                let visible_page = self.front_page;
                self.var.yoffset = target_page as u32 * self.var.yres;
                self.var.activate = FB_ACTIVATE_NOW;
                if unsafe {
                    libc::ioctl(self.file.as_raw_fd(), FBIOPAN_DISPLAY as _, &mut self.var)
                } == 0
                {
                    self.front_page = target_page;
                } else {
                    eprintln!("[video] page flip unavailable; using the visible page");
                    self.pages = 1;
                    self.front_page = visible_page;
                    self.var.yoffset = visible_page as u32 * self.var.yres;
                    self.var.activate = FB_ACTIVATE_NOW;
                    self.blit_to_page(frame, self.front_page);
                }
            }
            Ok(())
        }

        fn blit_to_page(&mut self, frame: &FrameSubmission, page: usize) {
            let offset = page * self.stride * self.var.yres as usize;
            let length = self.stride * self.var.yres as usize;
            if offset + length > self.mapping_len {
                return;
            }
            let destination =
                unsafe { std::slice::from_raw_parts_mut(self.mapping.add(offset), length) };
            let update_start = Instant::now();
            let exact_2x = self.var.xres as usize == frame.width as usize * 2
                && self.var.yres as usize == frame.height as usize * 2;
            let copied =
                if let Some(indices) = exact_2x.then(|| byte_indices(self.format)).flatten() {
                    if self.source_shadows[page].len() != frame.pixels.len() {
                        self.source_shadows[page].resize(frame.pixels.len(), 0);
                        self.page_valid[page] = false;
                    }
                    if !self.page_valid[page] {
                        if self.rotate_180 {
                            blit_2x_rotated(
                                &frame.pixels,
                                frame.width as usize,
                                frame.height as usize,
                                &mut self.staging,
                                self.stride,
                                indices,
                            );
                        } else {
                            blit_2x(
                                &frame.pixels,
                                frame.width as usize,
                                frame.height as usize,
                                &mut self.staging,
                                self.stride,
                                indices,
                            );
                        }
                        destination.copy_from_slice(&self.staging);
                        self.source_shadows[page].copy_from_slice(&frame.pixels);
                        self.page_valid[page] = true;
                        length
                    } else {
                        blit_changed_2x(
                            &frame.pixels,
                            &mut self.source_shadows[page],
                            &mut self.staging,
                            destination,
                            frame.width as usize,
                            frame.height as usize,
                            self.stride,
                            indices,
                            self.rotate_180,
                        )
                    }
                } else {
                    let direct = self.direct_present
                        && frame.width == self.var.xres
                        && frame.height == self.var.yres;
                    let target = if direct {
                        &mut *destination
                    } else {
                        &mut self.staging
                    };
                    blit_scaled(
                        &frame.pixels,
                        frame.width as usize,
                        frame.height as usize,
                        target,
                        self.var.xres as usize,
                        self.var.yres as usize,
                        self.stride,
                        self.format,
                        self.rotate_180,
                    );
                    if !direct {
                        destination.copy_from_slice(&self.staging);
                    }
                    length
                };
            let update_time = update_start.elapsed();
            self.present_count += 1;
            if self.present_count % 120 == 0 {
                eprintln!(
                    "[video] update={:.1}ms bytes={}",
                    update_time.as_secs_f64() * 1000.0,
                    copied
                );
            }
        }
    }

    impl Drop for Framebuffer {
        fn drop(&mut self) {
            unsafe {
                libc::munmap(self.mapping.cast(), self.mapping_len);
            }
        }
    }

    impl AsyncPresenter {
        fn new() -> Result<Self> {
            let mut device = Framebuffer::open()?;
            let queue = Arc::new(PresenterQueue {
                pending: Mutex::new(None),
                recycled: Mutex::new(Vec::with_capacity(1)),
                ready: Condvar::new(),
                error: Mutex::new(None),
                submitted: AtomicU64::new(0),
                displayed: AtomicU64::new(0),
            });
            let worker_queue = Arc::clone(&queue);
            std::thread::Builder::new()
                .name("framebuffer-presenter".to_owned())
                .spawn(move || loop {
                    let mut frame = {
                        let mut pending = worker_queue
                            .pending
                            .lock()
                            .expect("present queue lock poisoned");
                        while pending.is_none() {
                            pending = worker_queue
                                .ready
                                .wait(pending)
                                .expect("present queue lock poisoned");
                        }
                        let frame = pending.take().expect("pending frame disappeared");
                        worker_queue.ready.notify_all();
                        frame
                    };

                    if let Err(error) = device.present(&frame) {
                        *worker_queue
                            .error
                            .lock()
                            .expect("present error lock poisoned") = Some(format!("{error:#}"));
                        worker_queue.ready.notify_all();
                        break;
                    }
                    worker_queue.displayed.fetch_add(1, Ordering::Relaxed);

                    let mut recycled = worker_queue
                        .recycled
                        .lock()
                        .expect("present buffer pool lock poisoned");
                    if recycled.is_empty() {
                        recycled.push(std::mem::take(&mut frame.pixels));
                    }
                })
                .context("start framebuffer presenter")?;
            Ok(Self { queue })
        }

        fn submit(&self, frame: &PixelBuffer) -> Result<()> {
            if let Some(error) = self
                .queue
                .error
                .lock()
                .expect("present error lock poisoned")
                .as_ref()
            {
                bail!("framebuffer presenter stopped: {error}");
            }

            let mut pending = self
                .queue
                .pending
                .lock()
                .expect("present queue lock poisoned");
            while pending.is_some() {
                if let Some(error) = self
                    .queue
                    .error
                    .lock()
                    .expect("present error lock poisoned")
                    .as_ref()
                {
                    bail!("framebuffer presenter stopped: {error}");
                }
                let (next, timeout) = self
                    .queue
                    .ready
                    .wait_timeout(pending, std::time::Duration::from_secs(2))
                    .expect("present queue lock poisoned");
                pending = next;
                anyhow::ensure!(
                    !timeout.timed_out(),
                    "framebuffer presenter stopped accepting frames"
                );
            }
            let mut pixels = self
                .queue
                .recycled
                .lock()
                .expect("present buffer pool lock poisoned")
                .pop()
                .unwrap_or_else(|| Vec::with_capacity(frame.pixels.len()));
            pixels.resize(frame.pixels.len(), 0);
            pixels.copy_from_slice(&frame.pixels);
            *pending = Some(FrameSubmission {
                width: frame.width,
                height: frame.height,
                pixels,
            });
            self.queue.submitted.fetch_add(1, Ordering::Relaxed);
            drop(pending);
            self.queue.ready.notify_one();
            Ok(())
        }

        fn finish(&self) -> Result<()> {
            let start = Instant::now();
            let submitted = self.queue.submitted.load(Ordering::Relaxed);
            while self.queue.displayed.load(Ordering::Relaxed) < submitted {
                if let Some(error) = self
                    .queue
                    .error
                    .lock()
                    .expect("present error lock poisoned")
                    .as_ref()
                {
                    bail!("framebuffer presenter stopped: {error}");
                }
                anyhow::ensure!(
                    start.elapsed().as_secs() < 2,
                    "framebuffer presentation did not finish"
                );
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            eprintln!(
                "[video] submitted={submitted} displayed={}",
                self.queue.displayed.load(Ordering::Relaxed)
            );
            Ok(())
        }
    }

    static PRESENTER: LazyLock<std::result::Result<AsyncPresenter, String>> =
        LazyLock::new(|| AsyncPresenter::new().map_err(|error| format!("{error:#}")));

    pub fn present(frame: &PixelBuffer) -> Result<()> {
        match &*PRESENTER {
            Ok(presenter) => presenter.submit(frame),
            Err(error) => bail!("initialize framebuffer presenter: {error}"),
        }
    }

    pub fn finish() -> Result<()> {
        match &*PRESENTER {
            Ok(presenter) => presenter.finish(),
            Err(error) => bail!("initialize framebuffer presenter: {error}"),
        }
    }

    #[cfg(test)]
    mod queue_tests {
        include!("framebuffer_queue_tests.rs");
    }
}

#[cfg(any(target_os = "linux", test))]
#[inline]
fn channel(value: u8, field: BitField) -> u32 {
    if field.length == 0 {
        return 0;
    }
    let max = if field.length >= 32 {
        u32::MAX
    } else {
        (1u32 << field.length) - 1
    };
    ((value as u32 * max + 127) / 255) << field.offset
}

#[cfg(any(target_os = "linux", test))]
#[inline]
fn pack(r: u8, g: u8, b: u8, a: u8, format: PixelFormat) -> u32 {
    channel(r, format.red)
        | channel(g, format.green)
        | channel(b, format.blue)
        | channel(a, format.alpha)
}

#[cfg(any(target_os = "linux", test))]
fn byte_indices(format: PixelFormat) -> Option<[usize; 4]> {
    let fields = [format.red, format.green, format.blue, format.alpha];
    if fields
        .iter()
        .any(|field| field.length != 8 || field.offset % 8 != 0 || field.offset >= 32)
    {
        return None;
    }
    let indices = fields.map(|field| (field.offset / 8) as usize);
    let mut seen = [false; 4];
    for index in indices {
        if seen[index] {
            return None;
        }
        seen[index] = true;
    }
    Some(indices)
}

#[cfg(any(target_os = "linux", test))]
fn blit_2x(
    source: &[u8],
    source_width: usize,
    source_height: usize,
    destination: &mut [u8],
    stride: usize,
    indices: [usize; 4],
) {
    let row_bytes = source_width * 8;
    for y in 0..source_height {
        let source_row = y * source_width * 4;
        let first_row = y * 2 * stride;
        blit_2x_span(
            source,
            destination,
            source_row,
            first_row,
            0,
            source_width,
            indices,
        );
        let second_row = first_row + stride;
        let (before_second, from_second) = destination.split_at_mut(second_row);
        from_second[..row_bytes].copy_from_slice(&before_second[first_row..first_row + row_bytes]);
    }
}

#[cfg(any(target_os = "linux", test))]
fn blit_2x_rotated(
    source: &[u8],
    source_width: usize,
    source_height: usize,
    destination: &mut [u8],
    stride: usize,
    indices: [usize; 4],
) {
    let row_bytes = source_width * 8;
    for y in 0..source_height {
        let source_row = y * source_width * 4;
        let first_row = (source_height - 1 - y) * 2 * stride;
        blit_2x_span_rotated(
            source,
            destination,
            source_row,
            first_row,
            source_width,
            0,
            source_width,
            indices,
        );
        let second_row = first_row + stride;
        let (before_second, from_second) = destination.split_at_mut(second_row);
        from_second[..row_bytes].copy_from_slice(&before_second[first_row..first_row + row_bytes]);
    }
}

#[cfg(any(target_os = "linux", test))]
fn blit_2x_span(
    source: &[u8],
    destination: &mut [u8],
    source_row: usize,
    destination_row: usize,
    x_start: usize,
    x_end: usize,
    indices: [usize; 4],
) {
    let pixel_count = x_end - x_start;
    let source_offset = source_row + x_start * 4;
    let destination_offset = destination_row + x_start * 8;
    let source_end = source_offset + pixel_count * 4;
    let destination_end = destination_offset + pixel_count * 8;
    let source = &source[source_offset..source_end];
    let destination = &mut destination[destination_offset..destination_end];

    if indices == [0, 1, 2, 3] || indices == [2, 1, 0, 3] {
        let swap_red_blue = indices[0] == 2;
        let source = source.as_ptr();
        let destination = destination.as_mut_ptr();
        for pixel_index in 0..pixel_count {
            let source_offset = pixel_index * 4;
            let destination_offset = pixel_index * 8;
            // The slices above prove both unaligned accesses are in bounds.
            let mut pixel = unsafe {
                u32::from_le(std::ptr::read_unaligned(
                    source.add(source_offset).cast::<u32>(),
                ))
            };
            if swap_red_blue {
                pixel = (pixel & 0xff00_ff00)
                    | ((pixel & 0x0000_00ff) << 16)
                    | ((pixel & 0x00ff_0000) >> 16);
            }
            let pair = (pixel as u64) | ((pixel as u64) << 32);
            unsafe {
                std::ptr::write_unaligned(
                    destination.add(destination_offset).cast::<u64>(),
                    pair.to_le(),
                );
            }
        }
        return;
    }

    for pixel_index in 0..pixel_count {
        let source_offset = pixel_index * 4;
        let destination_offset = pixel_index * 8;
        let mut pair = [0u8; 8];
        pair[indices[0]] = source[source_offset];
        pair[indices[1]] = source[source_offset + 1];
        pair[indices[2]] = source[source_offset + 2];
        pair[indices[3]] = source[source_offset + 3];
        pair.copy_within(0..4, 4);
        destination[destination_offset..destination_offset + 8].copy_from_slice(&pair);
    }
}

#[cfg(any(target_os = "linux", test))]
fn blit_2x_span_rotated(
    source: &[u8],
    destination: &mut [u8],
    source_row: usize,
    destination_row: usize,
    source_width: usize,
    x_start: usize,
    x_end: usize,
    indices: [usize; 4],
) {
    let pixel_count = x_end - x_start;
    let destination_start = source_width - x_end;

    if indices == [0, 1, 2, 3] || indices == [2, 1, 0, 3] {
        let swap_red_blue = indices[0] == 2;
        let source = source.as_ptr();
        let destination = destination.as_mut_ptr();
        for output_index in 0..pixel_count {
            let source_x = x_end - 1 - output_index;
            let source_offset = source_row + source_x * 4;
            let destination_offset = destination_row + (destination_start + output_index) * 8;
            // The caller-provided source and destination spans prove these
            // unaligned accesses are in bounds.
            let mut pixel = unsafe {
                u32::from_le(std::ptr::read_unaligned(
                    source.add(source_offset).cast::<u32>(),
                ))
            };
            if swap_red_blue {
                pixel = (pixel & 0xff00_ff00)
                    | ((pixel & 0x0000_00ff) << 16)
                    | ((pixel & 0x00ff_0000) >> 16);
            }
            let pair = (pixel as u64) | ((pixel as u64) << 32);
            unsafe {
                std::ptr::write_unaligned(
                    destination.add(destination_offset).cast::<u64>(),
                    pair.to_le(),
                );
            }
        }
        return;
    }

    for output_index in 0..pixel_count {
        let source_x = x_end - 1 - output_index;
        let source_offset = source_row + source_x * 4;
        let destination_x = destination_start + output_index;
        let destination_offset = destination_row + destination_x * 8;
        let mut pair = [0u8; 8];
        pair[indices[0]] = source[source_offset];
        pair[indices[1]] = source[source_offset + 1];
        pair[indices[2]] = source[source_offset + 2];
        pair[indices[3]] = source[source_offset + 3];
        pair.copy_within(0..4, 4);
        destination[destination_offset..destination_offset + 8].copy_from_slice(&pair);
    }
}

#[cfg(any(target_os = "linux", test))]
fn native_present_enabled() -> bool {
    static ENABLED: std::sync::LazyLock<bool> =
        std::sync::LazyLock::new(|| std::env::var("BALATRO_NATIVE_PRESENT").as_deref() != Ok("0"));
    *ENABLED
}

#[cfg(any(target_os = "linux", test))]
fn blit_native(
    source: &[u8],
    width: usize,
    height: usize,
    destination: &mut [u8],
    stride: usize,
    indices: [usize; 4],
    rotate_180: bool,
) {
    let row_bytes = width * 4;
    for y in 0..height {
        let source_y = if rotate_180 { height - 1 - y } else { y };
        let source = &source[source_y * row_bytes..(source_y + 1) * row_bytes];
        let destination = &mut destination[y * stride..y * stride + row_bytes];
        if native_present_enabled() && matches!(indices, [0, 1, 2, 3] | [2, 1, 0, 3]) {
            sprite_to_text::pixel_copy::copy_rgba(source, destination, indices[0] == 2, rotate_180);
            continue;
        }
        if !rotate_180 && indices == [0, 1, 2, 3] {
            destination.copy_from_slice(source);
            continue;
        }
        if rotate_180 {
            copy_native_pixels(source.chunks_exact(4).rev(), destination, indices);
        } else {
            copy_native_pixels(source.chunks_exact(4), destination, indices);
        }
    }
}

#[cfg(any(target_os = "linux", test))]
fn copy_native_pixels<'a>(
    source: impl Iterator<Item = &'a [u8]>,
    destination: &mut [u8],
    indices: [usize; 4],
) {
    if indices == [2, 1, 0, 3] {
        for (source, destination) in source.zip(destination.chunks_exact_mut(4)) {
            destination.copy_from_slice(&[source[2], source[1], source[0], source[3]]);
        }
    } else {
        for (source, destination) in source.zip(destination.chunks_exact_mut(4)) {
            destination[indices[0]] = source[0];
            destination[indices[1]] = source[1];
            destination[indices[2]] = source[2];
            destination[indices[3]] = source[3];
        }
    }
}

#[cfg(any(target_os = "linux", test))]
fn blit_scaled(
    source: &[u8],
    source_width: usize,
    source_height: usize,
    destination: &mut [u8],
    destination_width: usize,
    destination_height: usize,
    stride: usize,
    format: PixelFormat,
    rotate_180: bool,
) {
    if native_present_enabled()
        && source_width == destination_width
        && source_height == destination_height
    {
        if let Some(indices) = byte_indices(format) {
            blit_native(
                source,
                source_width,
                source_height,
                destination,
                stride,
                indices,
                rotate_180,
            );
            // Every image pixel was overwritten. Only padding needs clearing.
            let row_bytes = source_width * 4;
            for y in 0..source_height {
                destination[y * stride + row_bytes..(y + 1) * stride].fill(0);
            }
            destination[source_height * stride..].fill(0);
            return;
        }
    }
    let destination_ratio = destination_width as u64 * source_height as u64;
    let source_ratio = source_width as u64 * destination_height as u64;
    let (view_width, view_height) = if destination_ratio <= source_ratio {
        (
            destination_width,
            destination_width * source_height / source_width.max(1),
        )
    } else {
        (
            destination_height * source_width / source_height.max(1),
            destination_height,
        )
    };
    let left = (destination_width - view_width) / 2;
    let top = (destination_height - view_height) / 2;
    destination.fill(0);

    if left == 0 && top == 0 && view_width == source_width * 2 && view_height == source_height * 2 {
        if let Some(indices) = byte_indices(format) {
            if rotate_180 {
                blit_2x_rotated(
                    source,
                    source_width,
                    source_height,
                    destination,
                    stride,
                    indices,
                );
            } else {
                blit_2x(
                    source,
                    source_width,
                    source_height,
                    destination,
                    stride,
                    indices,
                );
            }
            return;
        }
    }

    let byte_indices = byte_indices(format);
    if left == 0 && top == 0 && view_width == source_width && view_height == source_height {
        if let Some(indices) = byte_indices {
            blit_native(
                source,
                source_width,
                source_height,
                destination,
                stride,
                indices,
                rotate_180,
            );
            return;
        }
    }
    for y in 0..view_height {
        let sample_y = if rotate_180 { view_height - 1 - y } else { y };
        let source_y = sample_y * source_height / view_height.max(1);
        let source_row = source_y * source_width * 4;
        let destination_row = (top + y) * stride + left * 4;
        for x in 0..view_width {
            let sample_x = if rotate_180 { view_width - 1 - x } else { x };
            let source_x = sample_x * source_width / view_width.max(1);
            let source_offset = source_row + source_x * 4;
            let destination_offset = destination_row + x * 4;
            if let Some(indices) = byte_indices {
                destination[destination_offset + indices[0]] = source[source_offset];
                destination[destination_offset + indices[1]] = source[source_offset + 1];
                destination[destination_offset + indices[2]] = source[source_offset + 2];
                destination[destination_offset + indices[3]] = source[source_offset + 3];
            } else {
                let pixel = pack(
                    source[source_offset],
                    source[source_offset + 1],
                    source[source_offset + 2],
                    source[source_offset + 3],
                    format,
                );
                destination[destination_offset..destination_offset + 4]
                    .copy_from_slice(&pixel.to_ne_bytes());
            }
        }
    }
}

#[cfg(any(target_os = "linux", test))]
fn blit_changed_2x(
    current: &[u8],
    previous: &mut [u8],
    staging: &mut [u8],
    destination: &mut [u8],
    source_width: usize,
    source_height: usize,
    stride: usize,
    indices: [usize; 4],
    rotate_180: bool,
) -> usize {
    const BLOCK: usize = 16;
    let mut copied = 0;
    let source_stride = source_width * 4;
    for y in 0..source_height {
        let source_row = y * source_stride;
        let source_end = source_row + source_stride;
        let current_row = &current[source_row..source_end];
        let previous_row = &previous[source_row..source_end];
        if current_row == previous_row {
            continue;
        }

        let first = current_row
            .chunks(BLOCK)
            .zip(previous_row.chunks(BLOCK))
            .position(|(new, old)| new != old)
            .unwrap_or(0)
            * BLOCK;
        let last = current_row
            .rchunks(BLOCK)
            .zip(previous_row.rchunks(BLOCK))
            .position(|(new, old)| new != old)
            .map_or(source_stride, |index| source_stride - index * BLOCK);
        let x_start = first / 4;
        let x_end = last.div_ceil(4).min(source_width);
        let first_output_row = if rotate_180 {
            (source_height - 1 - y) * 2 * stride
        } else {
            y * 2 * stride
        };
        let (output_start, output_end) = if rotate_180 {
            blit_2x_span_rotated(
                current,
                staging,
                source_row,
                first_output_row,
                source_width,
                x_start,
                x_end,
                indices,
            );
            ((source_width - x_end) * 8, (source_width - x_start) * 8)
        } else {
            blit_2x_span(
                current,
                staging,
                source_row,
                first_output_row,
                x_start,
                x_end,
                indices,
            );
            (x_start * 8, x_end * 8)
        };
        let second_output_row = first_output_row + stride;
        let first_span = first_output_row + output_start..first_output_row + output_end;
        let second_span = second_output_row + output_start..second_output_row + output_end;
        staging.copy_within(first_span.clone(), second_output_row + output_start);
        destination[first_span.clone()].copy_from_slice(&staging[first_span]);
        destination[second_span.clone()].copy_from_slice(&staging[second_span]);
        previous[source_row + first..source_row + last]
            .copy_from_slice(&current[source_row + first..source_row + last]);
        copied += (output_end - output_start) * 2;
    }
    copied
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rgba() -> PixelFormat {
        PixelFormat {
            red: BitField {
                offset: 0,
                length: 8,
                msb_right: 0,
            },
            green: BitField {
                offset: 8,
                length: 8,
                msb_right: 0,
            },
            blue: BitField {
                offset: 16,
                length: 8,
                msb_right: 0,
            },
            alpha: BitField {
                offset: 24,
                length: 8,
                msb_right: 0,
            },
        }
    }

    #[test]
    fn native_copy_keeps_channels_rotation_padding_and_odd_sizes() {
        for (width, height) in [(1, 1), (3, 7), (17, 9), (640, 480)] {
            let source: Vec<u8> = (0..width * height * 4).map(|i| (i * 37) as u8).collect();
            let stride = width * 4 + 12;
            for indices in [[0, 1, 2, 3], [2, 1, 0, 3], [1, 2, 3, 0]] {
                for rotated in [false, true] {
                    let mut actual = vec![173; stride * height];
                    let mut expected = actual.clone();
                    blit_native(
                        &source,
                        width,
                        height,
                        &mut actual,
                        stride,
                        indices,
                        rotated,
                    );
                    for y in 0..height {
                        for x in 0..width {
                            let sx = if rotated { width - 1 - x } else { x };
                            let sy = if rotated { height - 1 - y } else { y };
                            for channel in 0..4 {
                                expected[y * stride + x * 4 + indices[channel]] =
                                    source[(sy * width + sx) * 4 + channel];
                            }
                        }
                    }
                    assert_eq!(
                        actual, expected,
                        "{width}x{height} {indices:?} rotated={rotated}"
                    );
                }
            }
        }
    }

    #[test]
    fn native_frame_clears_padding_and_near_native_letterboxes() {
        let source: Vec<u8> = (0..17 * 9 * 4).map(|i| (i * 13) as u8).collect();
        for (width, height) in [(17, 9), (18, 9), (17, 10)] {
            let stride = width * 4 + 13;
            for rotated in [false, true] {
                let mut actual = vec![173; stride * height + 21];
                blit_scaled(
                    &source,
                    17,
                    9,
                    &mut actual,
                    width,
                    height,
                    stride,
                    rgba(),
                    rotated,
                );
                let mut expected = vec![0; actual.len()];
                for y in 0..9 {
                    for x in 0..17 {
                        let sx = if rotated { 16 - x } else { x };
                        let sy = if rotated { 8 - y } else { y };
                        expected[y * stride + x * 4..y * stride + x * 4 + 4]
                            .copy_from_slice(&source[(sy * 17 + sx) * 4..(sy * 17 + sx) * 4 + 4]);
                    }
                }
                assert_eq!(actual, expected, "{width}x{height} rotated={rotated}");
            }
        }
    }

    #[test]
    fn native_scaled_path_keeps_the_framebuffer_layout() {
        let source: Vec<u8> = (0..5 * 7 * 4).map(|i| (i * 13) as u8).collect();
        for rotated in [false, true] {
            let mut output = vec![0; 32 * 7];
            blit_scaled(&source, 5, 7, &mut output, 5, 7, 32, rgba(), rotated);
            let mut expected = output.clone();
            blit_native(&source, 5, 7, &mut expected, 32, [0, 1, 2, 3], rotated);
            assert_eq!(output, expected);
        }
    }

    #[test]
    fn nearest_neighbor_scales_a_pixel_to_four_pixels() {
        let source = [10, 20, 30, 255];
        let mut destination = [0u8; 16];
        blit_scaled(&source, 1, 1, &mut destination, 2, 2, 8, rgba(), false);
        assert_eq!(&destination[0..4], &source);
        assert_eq!(&destination[4..8], &source);
        assert_eq!(&destination[8..12], &source);
        assert_eq!(&destination[12..16], &source);
    }

    #[test]
    fn two_times_scale_respects_bgra_layout() {
        let source = [10, 20, 30, 255];
        let mut destination = [0u8; 16];
        let field = |offset| BitField {
            offset,
            length: 8,
            msb_right: 0,
        };
        let bgra = PixelFormat {
            red: field(16),
            green: field(8),
            blue: field(0),
            alpha: field(24),
        };
        blit_scaled(&source, 1, 1, &mut destination, 2, 2, 8, bgra, false);
        assert_eq!(&destination[0..4], &[30, 20, 10, 255]);
        assert_eq!(&destination[12..16], &[30, 20, 10, 255]);
    }

    #[test]
    fn rotated_two_times_scale_respects_bgra_layout() {
        let first = [10, 20, 30, 255];
        let second = [40, 50, 60, 255];
        let source = [first, second].concat();
        let mut destination = [0u8; 32];
        let field = |offset| BitField {
            offset,
            length: 8,
            msb_right: 0,
        };
        let bgra = PixelFormat {
            red: field(16),
            green: field(8),
            blue: field(0),
            alpha: field(24),
        };

        blit_scaled(&source, 2, 1, &mut destination, 4, 2, 16, bgra, true);

        let expected_row = [
            [60, 50, 40, 255],
            [60, 50, 40, 255],
            [30, 20, 10, 255],
            [30, 20, 10, 255],
        ]
        .concat();
        assert_eq!(&destination[..16], expected_row.as_slice());
        assert_eq!(&destination[16..], expected_row.as_slice());
    }

    #[test]
    fn two_times_scale_keeps_generic_byte_layouts() {
        let source = [10, 20, 30, 40, 50, 60, 70, 80];
        let mut destination = [0u8; 32];
        blit_2x_span(&source, &mut destination, 0, 0, 0, 2, [1, 2, 3, 0]);
        assert_eq!(
            destination,
            [
                40, 10, 20, 30, 40, 10, 20, 30, 80, 50, 60, 70, 80, 50, 60, 70, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0
            ]
        );
    }

    #[test]
    fn dirty_copy_leaves_unchanged_rows_alone() {
        let mut current = vec![0u8; 64];
        let mut previous = vec![0u8; 64];
        let mut staging = vec![0u8; 256];
        let mut destination = vec![9u8; 256];
        current[40..44].copy_from_slice(&[1, 2, 3, 4]);

        let copied = blit_changed_2x(
            &current,
            &mut previous,
            &mut staging,
            &mut destination,
            8,
            2,
            64,
            [0, 1, 2, 3],
            false,
        );

        assert_eq!(copied, 64);
        assert_eq!(&destination[..128], &[9u8; 128]);
        assert_eq!(&destination[144..148], &[1, 2, 3, 4]);
        assert_eq!(&destination[208..212], &[1, 2, 3, 4]);
        assert_eq!(&destination[160..192], &[9u8; 32]);
        assert_eq!(previous, current);
    }

    #[test]
    fn two_times_scale_can_rotate_180_degrees() {
        let red = [255, 0, 0, 255];
        let green = [0, 255, 0, 255];
        let source = [red, green].concat();
        let mut destination = [0u8; 32];

        blit_scaled(&source, 2, 1, &mut destination, 4, 2, 16, rgba(), true);

        let expected_row = [green, green, red, red].concat();
        assert_eq!(&destination[..16], expected_row.as_slice());
        assert_eq!(&destination[16..], expected_row.as_slice());
    }

    #[test]
    fn dirty_copy_rotates_changed_pixels() {
        let red = [255, 0, 0, 255];
        let green = [0, 255, 0, 255];
        let current = [red, green].concat();
        let mut previous = vec![0u8; current.len()];
        let mut staging = vec![0u8; 32];
        let mut destination = vec![9u8; 32];

        let copied = blit_changed_2x(
            &current,
            &mut previous,
            &mut staging,
            &mut destination,
            2,
            1,
            16,
            [0, 1, 2, 3],
            true,
        );

        let expected_row = [green, green, red, red].concat();
        assert_eq!(copied, 32);
        assert_eq!(&destination[..16], expected_row.as_slice());
        assert_eq!(&destination[16..], expected_row.as_slice());
        assert_eq!(previous, current);
    }
}

#[cfg(target_os = "linux")]
pub fn present(frame: &PixelBuffer) -> Result<()> {
    linux::present(frame)
}

#[cfg(not(target_os = "linux"))]
pub fn present(_frame: &PixelBuffer) -> Result<()> {
    anyhow::bail!("the framebuffer backend is only available on Linux")
}

pub fn finish() -> Result<()> {
    #[cfg(target_os = "linux")]
    return linux::finish();
    #[cfg(not(target_os = "linux"))]
    Ok(())
}
