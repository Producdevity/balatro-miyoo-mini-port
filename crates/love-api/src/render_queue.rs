use crate::occlusion::{Coverage, Rect};
use parking_lot::Mutex;
use sprite_to_text::pixel_buffer::PixelBuffer;
use std::sync::{mpsc, Arc};

pub(crate) struct RenderJob {
    draw: Draw,
    coverage: Coverage,
}

enum Draw {
    Function(Box<dyn Fn(&mut PixelBuffer, Option<Rect>) + Send + 'static>),
    #[cfg(feature = "layer-pairs")]
    Image(Box<crate::graphics::ImageDraw>),
}

impl RenderJob {
    #[cfg(test)]
    pub(crate) fn new(draw: impl Fn(&mut PixelBuffer) + Send + 'static) -> Self {
        Self::with_coverage(Coverage::default(), move |buffer, _| draw(buffer))
    }

    pub(crate) fn with_coverage(
        coverage: Coverage,
        draw: impl Fn(&mut PixelBuffer, Option<Rect>) + Send + 'static,
    ) -> Self {
        Self {
            draw: Draw::Function(Box::new(draw)),
            coverage,
        }
    }

    pub(crate) fn run(&self, buffer: &mut PixelBuffer, hidden: Option<Rect>) {
        match &self.draw {
            Draw::Function(draw) => draw(buffer, hidden),
            #[cfg(feature = "layer-pairs")]
            Draw::Image(draw) => draw.run(buffer, hidden),
        }
    }

    #[cfg(feature = "layer-pairs")]
    pub(crate) fn image(coverage: Coverage, draw: crate::graphics::ImageDraw) -> Self {
        Self {
            coverage,
            draw: Draw::Image(Box::new(draw)),
        }
    }

    #[cfg(feature = "layer-pairs")]
    fn draw_pair(&self, later: &Self, buffer: &mut PixelBuffer, later_jobs: &[RenderJob]) -> bool {
        static ENABLED: std::sync::LazyLock<bool> =
            std::sync::LazyLock::new(|| std::env::var("BALATRO_LAYER_PAIRS").as_deref() == Ok("1"));
        if !*ENABLED {
            return false;
        }
        let (Some(base), Some(overlay), Some(bounds)) = (
            self.coverage.layer,
            later.coverage.layer,
            self.coverage.bounds,
        ) else {
            return false;
        };
        if !overlay.follows(base) {
            return false;
        }
        match (&self.draw, &later.draw) {
            (Draw::Image(first), Draw::Image(second)) => first.draw_pair(
                second,
                base,
                bounds,
                buffer,
                hidden_by(self.coverage, later_jobs),
            ),
            _ => false,
        }
    }
}

#[derive(Default)]
struct OcclusionStats {
    candidates: u64,
    clipped: u64,
    bounds_pixels: u64,
    hidden_pixels: u64,
    discarded_layers: u64,
    #[cfg(feature = "layer-pairs")]
    paired_layers: u64,
    #[cfg(feature = "native-sampler")]
    layers: crate::card_layers::Probe,
}

fn draw_batch(jobs: &mut Vec<RenderJob>, buffer: &mut PixelBuffer, stats: &mut OcclusionStats) {
    #[cfg(feature = "layer-pairs")]
    let mut paired = false;
    for (index, job) in jobs.iter().enumerate() {
        #[cfg(feature = "native-sampler")]
        stats
            .layers
            .observe(job.coverage.layer, job.coverage.bounds);
        #[cfg(feature = "layer-pairs")]
        if std::mem::take(&mut paired) {
            continue;
        }
        if jobs[index + 1..]
            .iter()
            .any(|later| job.coverage.covered_by(later.coverage))
        {
            stats.discarded_layers += 1;
            continue;
        }
        #[cfg(feature = "layer-pairs")]
        if jobs
            .get(index + 1)
            .is_some_and(|later| job.draw_pair(later, buffer, &jobs[index + 2..]))
        {
            paired = true;
            stats.paired_layers += 1;
            continue;
        }
        let hidden = hidden_by(job.coverage, &jobs[index + 1..]);
        if let Some(bounds) = job.coverage.bounds {
            stats.candidates += 1;
            stats.bounds_pixels += bounds.area() as u64;
        }
        if let Some(hidden) = hidden {
            stats.clipped += 1;
            stats.hidden_pixels += hidden.area() as u64;
        }
        job.run(buffer, hidden);
    }
    jobs.clear();
}

fn hidden_by(coverage: Coverage, later: &[RenderJob]) -> Option<Rect> {
    coverage.bounds.and_then(|bounds| {
        later
            .iter()
            .filter_map(|job| job.coverage.opaque)
            .filter_map(|opaque| bounds.intersect(opaque))
            .max_by_key(|rect| rect.area())
            .filter(|rect| rect.area() >= 128)
    })
}

enum Message {
    Draw(RenderJob),
    Boundary(Box<dyn FnOnce(&mut PixelBuffer) + Send>),
    Flush(mpsc::Sender<()>),
    Stop,
}

struct QueueState {
    pending: bool,
    reference: Option<PixelBuffer>,
    checks: u64,
    #[cfg(feature = "queue-profile")]
    waits: crate::render_waits::RenderWaits,
}

pub(crate) struct RenderQueue {
    sender: mpsc::Sender<Message>,
    thread: Option<std::thread::JoinHandle<()>>,
    buffer: Arc<Mutex<PixelBuffer>>,
    state: Mutex<QueueState>,
}

impl RenderQueue {
    pub(crate) fn new(buffer: Arc<Mutex<PixelBuffer>>) -> anyhow::Result<Self> {
        let verify = std::env::var("BALATRO_VERIFY_RASTER").as_deref() == Ok("1");
        Self::with_verification(buffer, verify)
    }

    fn with_verification(buffer: Arc<Mutex<PixelBuffer>>, verify: bool) -> anyhow::Result<Self> {
        let reference = verify.then(|| {
            let source = buffer.lock();
            PixelBuffer::new(source.width, source.height)
        });
        let (sender, receiver) = mpsc::channel();
        let target = Arc::clone(&buffer);
        let thread = std::thread::Builder::new()
            .name("software-raster".into())
            .stack_size(512 * 1024)
            .spawn(move || {
                let occlusion = crate::occlusion::enabled();
                let mut batch = Vec::with_capacity(if occlusion { 64 } else { 0 });
                let mut stats = OcclusionStats::default();
                #[cfg(all(feature = "native-sampler", target_os = "linux", target_arch = "arm"))]
                let _sampler = crate::native_sampler::Sampler::for_thread("raster")
                    .map_err(|error| eprintln!("[native-sample] unavailable: {error}"))
                    .ok()
                    .flatten();
                'worker: while let Ok(message) = receiver.recv() {
                    match message {
                        Message::Draw(job) => {
                            let mut buffer = target.lock();
                            if occlusion {
                                batch.push(job);
                            } else {
                                job.run(&mut buffer, None);
                            }
                            loop {
                                match receiver.try_recv() {
                                    Ok(Message::Draw(job)) => {
                                        if occlusion {
                                            batch.push(job);
                                            if batch.len() == 64 {
                                                draw_batch(&mut batch, &mut buffer, &mut stats);
                                            }
                                        } else {
                                            job.run(&mut buffer, None);
                                        }
                                    }
                                    Ok(Message::Boundary(action)) => {
                                        draw_batch(&mut batch, &mut buffer, &mut stats);
                                        #[cfg(feature = "native-sampler")]
                                        stats.layers.boundary();
                                        action(&mut buffer);
                                    }
                                    Ok(Message::Flush(done)) => {
                                        draw_batch(&mut batch, &mut buffer, &mut stats);
                                        drop(buffer);
                                        let _ = done.send(());
                                        continue 'worker;
                                    }
                                    Ok(Message::Stop) => {
                                        draw_batch(&mut batch, &mut buffer, &mut stats);
                                        break 'worker;
                                    }
                                    Err(mpsc::TryRecvError::Empty) => {
                                        draw_batch(&mut batch, &mut buffer, &mut stats);
                                        break;
                                    }
                                    Err(mpsc::TryRecvError::Disconnected) => {
                                        draw_batch(&mut batch, &mut buffer, &mut stats);
                                        break 'worker;
                                    }
                                }
                            }
                        }
                        Message::Boundary(action) => {
                            #[cfg(feature = "native-sampler")]
                            stats.layers.boundary();
                            action(&mut target.lock());
                        }
                        Message::Flush(done) => {
                            let _ = done.send(());
                        }
                        Message::Stop => break,
                    }
                }
                if occlusion {
                    eprintln!(
                        "[occlusion] candidates={} clipped={} bounds_pixels={} hidden_pixels={} discarded_layers={}",
                        stats.candidates, stats.clipped, stats.bounds_pixels, stats.hidden_pixels,
                        stats.discarded_layers
                    );
                }
                #[cfg(feature = "layer-pairs")]
                eprintln!("[layer-pairs] combined={}", stats.paired_layers);
            })?;
        Ok(Self {
            sender,
            thread: Some(thread),
            buffer,
            state: Mutex::new(QueueState {
                pending: false,
                reference,
                checks: 0,
                #[cfg(feature = "queue-profile")]
                waits: Default::default(),
            }),
        })
    }

    pub(crate) fn submit(&self, job: RenderJob) {
        let mut state = self.state.lock();
        if !state.pending {
            if let Some(reference) = &mut state.reference {
                reference.copy_raster_source(&self.buffer.lock());
            }
        }
        if let Some(reference) = &mut state.reference {
            job.run(reference, None);
        }
        assert!(
            self.sender.send(Message::Draw(job)).is_ok(),
            "raster worker stopped"
        );
        state.pending = true;
    }

    #[cfg_attr(feature = "queue-profile", track_caller)]
    pub(crate) fn flush(&self) {
        let mut state = self.state.lock();
        if !state.pending {
            return;
        }
        #[cfg(feature = "queue-profile")]
        let started = std::time::Instant::now();
        let (done, wait) = mpsc::channel();
        assert!(
            self.sender.send(Message::Flush(done)).is_ok(),
            "raster worker stopped"
        );
        wait.recv().expect("raster worker stopped before flush");
        #[cfg(feature = "queue-profile")]
        {
            let caller = std::panic::Location::caller();
            state
                .waits
                .record(caller.file(), caller.line(), started.elapsed());
        }
        if let Some(reference) = &state.reference {
            let target = self.buffer.lock();
            assert_eq!(target.pixels.len(), reference.pixels.len());
            if let Some((index, (actual, expected))) = target
                .pixels
                .iter()
                .zip(&reference.pixels)
                .enumerate()
                .find(|(_, (a, b))| a != b)
            {
                panic!("raster mismatch at byte {index}: {actual} != {expected}");
            }
            state.checks += 1;
        }
        state.pending = false;
    }

    pub(crate) fn after_draws(&self, action: impl FnOnce(&mut PixelBuffer) + Send + 'static) {
        let verifying = self.state.lock().reference.is_some();
        if verifying {
            self.flush();
        }
        let mut state = self.state.lock();
        self.sender
            .send(Message::Boundary(Box::new(action)))
            .expect("raster worker stopped");
        state.pending = true;
        // Output is a side effect, not a replayable draw. In verification mode,
        // finish it once before the next batch takes its reference snapshot.
        if verifying {
            let (done, wait) = mpsc::channel();
            self.sender
                .send(Message::Flush(done))
                .expect("raster worker stopped");
            wait.recv().expect("raster worker stopped before output");
            state.pending = false;
        }
    }
}

impl Drop for RenderQueue {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            self.flush();
            let state = self.state.lock();
            #[cfg(feature = "queue-profile")]
            state.waits.report();
            if state.reference.is_some() {
                eprintln!(
                    "[raster-check] {} batches matched sequential rendering",
                    state.checks
                );
            }
        }
        let _ = self.sender.send(Message::Stop);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn later_opaque_effect_replaces_a_binary_alpha_base_layer() {
        use crate::occlusion::OpacityCache;
        use crate::state::{ImageData, Transform};
        use sprite_to_text::pixel_buffer::DissolveParams;
        let mut source = ImageData {
            width: 19,
            height: 17,
            pixels: vec![255; 19 * 17 * 4],
            white_alpha_mask: false,
        };
        for (n, pixel) in source.pixels.chunks_exact_mut(4).enumerate() {
            pixel.copy_from_slice(&[
                n as u8,
                (n * 17) as u8,
                (n * 53) as u8,
                if n % 3 == 0 { 0 } else { 255 },
            ]);
        }
        let image = Arc::new(source);
        for effect in [0, 4, 5, 6] {
            for angle in [-0.35, 0.015, 0.21] {
                let mut transform = Transform::default();
                transform.translate(31.7, 27.3);
                transform.rotate(angle);
                transform.scale(1.7, 1.4);
                let inv = transform.inverse().unwrap();
                let mut cache = OpacityCache::default();
                let mut jobs = Vec::new();
                for layer in 0..3 {
                    if layer == 1 {
                        jobs.push(RenderJob::new(|buffer| {
                            buffer.fill_rect(20, 15, 60, 60, [113, 37, 193, 111])
                        }));
                        continue;
                    }
                    let params = DissolveParams {
                        shader_effect: if layer == 0 { 0 } else { effect },
                        ..DissolveParams::NONE
                    };
                    let coverage = cache.coverage(
                        &image,
                        [0.0, 0.0, 19.0, 17.0],
                        &transform,
                        params,
                        255,
                        0,
                        false,
                        Rect([0, 0, 96, 80]),
                    );
                    let image = Arc::clone(&image);
                    jobs.push(RenderJob::with_coverage(coverage, move |buffer, _| {
                        buffer.draw_image_region_transformed(
                            &image.pixels,
                            19,
                            0.0,
                            0.0,
                            19.0,
                            17.0,
                            (0, 0, 96, 80),
                            [inv.a, inv.b, inv.tx, inv.c, inv.d, inv.ty],
                            [255; 4],
                            false,
                            false,
                            params,
                        );
                    }));
                }
                let mut actual = PixelBuffer::new(96, 80);
                actual.clear(0.3, 0.6, 0.2, 0.7);
                let mut expected = PixelBuffer::new(96, 80);
                expected.copy_raster_source(&actual);
                for job in &jobs {
                    job.run(&mut expected, None);
                }
                let mut stats = OcclusionStats::default();
                draw_batch(&mut jobs, &mut actual, &mut stats);
                assert_eq!(stats.discarded_layers, 1);
                assert_eq!(
                    actual.pixels, expected.pixels,
                    "effect={effect} angle={angle}"
                );
            }
        }
    }

    #[test]
    fn occlusion_matches_layered_shaders_with_clipping_and_rotation() {
        use crate::occlusion::{draw_visible, OpacityCache};
        use crate::state::{ImageData, Transform};
        use sprite_to_text::pixel_buffer::DissolveParams;
        let mut source = ImageData {
            width: 71,
            height: 95,
            pixels: vec![255; 71 * 95 * 4],
            white_alpha_mask: false,
        };
        for (n, pixel) in source.pixels.chunks_exact_mut(4).enumerate() {
            pixel[..3].copy_from_slice(&[n as u8, (n * 17) as u8, (n * 53) as u8]);
            if n % 71 < 2 || n % 71 > 68 || n / 71 < 2 || n / 71 > 92 {
                pixel[3] = 128;
            }
        }
        let source = Arc::new(source);
        let region = [0.0, 0.0, 71.0, 95.0];
        let mut cache = OpacityCache::default();
        for effect in 0..=11 {
            for frame in 0..8 {
                let mut jobs = Vec::new();
                for layer in 0..3 {
                    if layer == 1 {
                        jobs.push(RenderJob::new(|buffer| {
                            buffer.fill_rect(30, 30, 60, 60, [21, 127, 197, 63])
                        }));
                        continue;
                    }
                    let mut t = Transform::default();
                    t.translate(20.1 + layer as f32 * 12.0, 20.7);
                    t.rotate((frame as f32 - 4.25) * 0.03);
                    t.scale(1.2, 1.1);
                    let params = DissolveParams {
                        shader_effect: if layer == 0 {
                            effect
                        } else {
                            [0, 4, 5, 6][frame % 4]
                        },
                        dissolve: if layer == 0 && frame > 4 { 0.3 } else { 0.0 },
                        ..DissolveParams::NONE
                    };
                    let coverage = cache.coverage(
                        &source,
                        region,
                        &t,
                        params,
                        255,
                        0,
                        false,
                        Rect([7, 9, 150, 148]),
                    );
                    let inverse = t.inverse().unwrap();
                    let image = Arc::clone(&source);
                    jobs.push(RenderJob::with_coverage(coverage, move |buffer, hidden| {
                        buffer.scissor = Some((7, 9, 143, 139));
                        draw_visible(buffer, hidden, |buffer| {
                            buffer.draw_image_region_transformed(
                                &image.pixels,
                                71,
                                0.0,
                                0.0,
                                71.0,
                                95.0,
                                (0, 0, 160, 160),
                                [
                                    inverse.a, inverse.b, inverse.tx, inverse.c, inverse.d,
                                    inverse.ty,
                                ],
                                [255; 4],
                                false,
                                false,
                                params,
                            )
                        });
                    }));
                }
                let mut expected = PixelBuffer::new(160, 160);
                let mut actual = PixelBuffer::new(160, 160);
                for job in &jobs {
                    job.run(&mut expected, None);
                }
                let mut stats = OcclusionStats::default();
                draw_batch(&mut jobs, &mut actual, &mut stats);
                assert_eq!(stats.clipped, 1);
                assert_eq!(
                    actual.pixels, expected.pixels,
                    "effect={effect} frame={frame}"
                );
            }
        }
    }

    #[test]
    fn queue_preserves_order_across_flushes_and_synchronous_changes() {
        let target = Arc::new(Mutex::new(PixelBuffer::new(9, 5)));
        let queue = RenderQueue::with_verification(Arc::clone(&target), true).unwrap();
        let mut reference = PixelBuffer::new(9, 5);
        for frame in 0..24 {
            target.lock().fill_rect(1, 1, 5, 3, [5, 33, 17, 128]);
            reference.fill_rect(1, 1, 5, 3, [5, 33, 17, 128]);
            for alpha in [0, 17, 127, 255] {
                let job = RenderJob::new(move |buffer| {
                    buffer.blend = frame % 6;
                    buffer.scissor = (frame % 2 == 0).then_some((3, 1, 3, 2));
                    buffer.fill_rect(0, 0, 9, 5, [23, 113, 181, alpha]);
                });
                job.run(&mut reference, None);
                queue.submit(job);
            }
            queue.flush();
            queue.flush();
            assert_eq!(target.lock().pixels, reference.pixels, "frame={frame}");
        }
        assert_eq!(queue.state.lock().checks, 24);
    }

    #[test]
    fn verification_tracks_resizes_and_empty_buffers() {
        let target = Arc::new(Mutex::new(PixelBuffer::new(0, 0)));
        let queue = RenderQueue::with_verification(Arc::clone(&target), true).unwrap();
        for (width, height) in [(0, 0), (1, 1), (8, 7), (9, 7), (3, 2)] {
            target.lock().resize(width, height);
            queue.submit(RenderJob::new(|buffer| {
                buffer.fill_rect(0, 0, 100, 100, [1, 2, 3, 255])
            }));
            queue.flush();
            assert_eq!(
                target.lock().pixels,
                [1, 2, 3, 255].repeat((width * height) as usize)
            );
        }
    }

    #[test]
    fn shutdown_finishes_pending_draws() {
        let target = Arc::new(Mutex::new(PixelBuffer::new(9, 5)));
        let queue = RenderQueue::with_verification(Arc::clone(&target), true).unwrap();
        queue.submit(RenderJob::new(|buffer| {
            buffer.fill_rect(0, 0, 9, 5, [7, 11, 19, 255])
        }));
        drop(queue);
        assert_eq!(target.lock().pixels, [7, 11, 19, 255].repeat(45));
    }

    #[test]
    fn output_boundaries_preserve_frames_and_run_once() {
        for verify in [false, true] {
            let target = Arc::new(Mutex::new(PixelBuffer::new(9, 5)));
            let queue = RenderQueue::with_verification(target, verify).unwrap();
            let (output, frames) = mpsc::channel();
            for frame in 0..24u8 {
                queue.submit(RenderJob::new(move |buffer| {
                    buffer.clear(0.0, 0.0, 0.0, 0.0);
                    buffer.fill_rect(0, 0, 9, 5, [frame, 11, 19, 255]);
                }));
                let output = output.clone();
                queue.after_draws(move |buffer| {
                    output.send(buffer.pixels.clone()).unwrap();
                    buffer.clear(1.0, 0.0, 0.0, 1.0);
                });
            }
            drop(queue);
            drop(output);
            let frames: Vec<_> = frames.iter().collect();
            assert_eq!(frames.len(), 24);
            for (frame, pixels) in frames.iter().enumerate() {
                assert_eq!(*pixels, [frame as u8, 11, 19, 255].repeat(45));
            }
        }
    }

    #[test]
    fn queued_pixels_survive_collection_of_their_lua_owner() {
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::time::Duration;

        struct LuaPixels {
            _pixels: Arc<[u8; 4]>,
            dropped: Arc<AtomicBool>,
        }
        impl mlua::UserData for LuaPixels {}
        impl Drop for LuaPixels {
            fn drop(&mut self) {
                self.dropped.store(true, Ordering::SeqCst);
            }
        }

        let lua = mlua::Lua::new();
        let pixels = Arc::new([7, 11, 19, 255]);
        let weak = Arc::downgrade(&pixels);
        let dropped = Arc::new(AtomicBool::new(false));
        lua.globals()
            .set(
                "image",
                lua.create_userdata(LuaPixels {
                    _pixels: Arc::clone(&pixels),
                    dropped: Arc::clone(&dropped),
                })
                .unwrap(),
            )
            .unwrap();
        let target = Arc::new(Mutex::new(PixelBuffer::new(1, 1)));
        let queue = RenderQueue::with_verification(Arc::clone(&target), false).unwrap();
        let (started, ready) = mpsc::channel();
        let (release, wait) = mpsc::channel();
        queue.submit(RenderJob::new(move |buffer| {
            started.send(()).unwrap();
            wait.recv_timeout(Duration::from_secs(5)).unwrap();
            buffer.pixels.copy_from_slice(pixels.as_ref());
        }));
        ready.recv_timeout(Duration::from_secs(5)).unwrap();

        lua.globals().set("image", mlua::Nil).unwrap();
        lua.gc_collect().unwrap();
        lua.gc_collect().unwrap();
        let lua_owner_collected = dropped.load(Ordering::SeqCst);
        let draw_still_owns_pixels = weak.upgrade().is_some();
        release.send(()).unwrap();
        queue.flush();
        drop(queue);

        assert!(lua_owner_collected);
        assert!(draw_still_owns_pixels);
        assert_eq!(target.lock().pixels, [7, 11, 19, 255]);
        assert!(weak.upgrade().is_none());
    }
}
