// Derived from balatro-port-tui (Apache-2.0). Modified by Producdevity; see NOTICE.

use super::*;

/// Query terminal window pixel dimensions via platform-specific API.
/// Returns (pixel_width, pixel_height) of the console window client area.
#[cfg(windows)]
pub(super) fn get_console_pixel_size() -> Option<(u32, u32)> {
    #[repr(C)]
    struct RECT {
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
    }
    extern "system" {
        fn GetConsoleWindow() -> isize;
        fn GetClientRect(hwnd: isize, rect: *mut RECT) -> i32;
    }
    unsafe {
        let hwnd = GetConsoleWindow();
        if hwnd == 0 {
            return None;
        }
        let mut rect = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        if GetClientRect(hwnd, &mut rect) != 0 {
            let w = (rect.right - rect.left) as u32;
            let h = (rect.bottom - rect.top) as u32;
            if w >= 100 && h >= 50 {
                return Some((w, h));
            }
        }
    }
    None
}

#[cfg(not(windows))]
pub(super) fn get_console_pixel_size() -> Option<(u32, u32)> {
    None
}

/// Get cell pixel size and raw text area dimensions from the foreground window (Windows Terminal).
/// Returns (cell_w, cell_h, text_area_w, text_area_h, hwnd, tab_bar_h).
/// Uses SetThreadDpiAwarenessContext for DPI-correct values.
#[cfg(windows)]
pub(super) fn get_foreground_window_cell_size(
    cols: u16,
    rows: u16,
) -> Option<(u16, u16, u32, u32, isize, u32)> {
    #[repr(C)]
    struct RECT {
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
    }
    extern "system" {
        fn GetForegroundWindow() -> isize;
        fn GetClientRect(hwnd: isize, rect: *mut RECT) -> i32;
        fn SetThreadDpiAwarenessContext(value: isize) -> isize;
        fn GetDpiForWindow(hwnd: isize) -> u32;
        fn LoadLibraryA(name: *const u8) -> isize;
        fn GetProcAddress(module: isize, name: *const u8) -> isize;
    }

    if cols == 0 || rows == 0 {
        return None;
    }

    unsafe {
        // Temporarily make this thread per-monitor DPI aware v2.
        // This ensures GetClientRect returns PHYSICAL pixels, not virtual.
        let old_ctx = SetThreadDpiAwarenessContext(-4); // DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2
        let thread_dpi_ok = old_ctx != 0;

        let hwnd = GetForegroundWindow();
        if hwnd == 0 {
            if thread_dpi_ok {
                SetThreadDpiAwarenessContext(old_ctx);
            }
            return None;
        }

        let dpi = GetDpiForWindow(hwnd);

        // GetClientRect returns physical pixels when thread is DPI-aware
        let mut client = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        let ok = GetClientRect(hwnd, &mut client);

        // Also try MonitorFromWindow + GetDpiForMonitor as backup DPI source
        let monitor_dpi = {
            extern "system" {
                fn MonitorFromWindow(hwnd: isize, flags: u32) -> isize;
            }
            // Dynamically load GetDpiForMonitor from shcore.dll
            let hmon = MonitorFromWindow(hwnd, 1); // MONITOR_DEFAULTTONEAREST
            if hmon != 0 {
                type GetDpiForMonitorFn =
                    unsafe extern "system" fn(isize, u32, *mut u32, *mut u32) -> i32;
                let lib = LoadLibraryA(b"shcore.dll\0".as_ptr());
                if lib != 0 {
                    let proc = GetProcAddress(lib, b"GetDpiForMonitor\0".as_ptr());
                    if proc != 0 {
                        let func: GetDpiForMonitorFn = std::mem::transmute(proc);
                        let mut dpi_x: u32 = 0;
                        let mut dpi_y: u32 = 0;
                        let hr = func(hmon, 0, &mut dpi_x, &mut dpi_y); // MDT_EFFECTIVE_DPI
                        if hr == 0 && dpi_x >= 96 {
                            Some(dpi_x)
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            }
        };

        // Restore previous DPI awareness context
        if thread_dpi_ok {
            SetThreadDpiAwarenessContext(old_ctx);
        }

        if ok == 0 {
            return None;
        }
        let client_w = (client.right - client.left) as u32;
        let client_h = (client.bottom - client.top) as u32;
        if client_w < 100 || client_h < 50 {
            return None;
        }

        // Use the best DPI value we found
        let effective_dpi = if dpi > 96 {
            dpi
        } else if let Some(md) = monitor_dpi {
            md
        } else {
            dpi
        };

        // Subtract approximate WT tab bar height (scales with DPI)
        let tab_bar_h = (32.0 * effective_dpi as f32 / 96.0) as u32;
        let text_h = client_h.saturating_sub(tab_bar_h);
        if text_h < 50 {
            return None;
        }

        let cell_w = (client_w / cols as u32) as u16;
        let cell_h = (text_h / rows as u32) as u16;

        eprintln!(
            "[INFO] ForegroundWindow: thread_dpi_ok={}, client={}x{}, dpi_window={}, dpi_monitor={:?}, tab≈{}px, text_h={}, cell={}x{}",
            thread_dpi_ok, client_w, client_h, dpi, monitor_dpi, tab_bar_h, text_h, cell_w, cell_h
        );

        if cell_w >= 4 && cell_w <= 30 && cell_h >= 8 && cell_h <= 60 {
            Some((cell_w, cell_h, client_w, text_h, hwnd, tab_bar_h))
        } else {
            None
        }
    }
}

#[cfg(not(windows))]
pub(super) fn get_foreground_window_cell_size(
    _cols: u16,
    _rows: u16,
) -> Option<(u16, u16, u32, u32, isize, u32)> {
    None
}

/// Query console font cell size via GetCurrentConsoleFontEx.
/// Returns (cell_width, cell_height) in pixels.
/// Works on classic Windows Console; may return conpty defaults on Windows Terminal.
#[cfg(windows)]
pub(super) fn get_console_font_size() -> Option<(u16, u16)> {
    #[repr(C)]
    struct COORD {
        x: i16,
        y: i16,
    }
    #[repr(C)]
    struct CONSOLE_FONT_INFOEX {
        cb_size: u32,
        font_index: u32,
        font_size: COORD,
        font_family: u32,
        font_weight: u32,
        face_name: [u16; 32],
    }
    extern "system" {
        fn GetStdHandle(n_std_handle: u32) -> isize;
        fn GetCurrentConsoleFontEx(
            h_console_output: isize,
            b_maximum_window: i32,
            lp_console_current_font_ex: *mut CONSOLE_FONT_INFOEX,
        ) -> i32;
    }
    unsafe {
        let handle = GetStdHandle(0xFFFF_FFF5_u32); // STD_OUTPUT_HANDLE
        if handle == 0 || handle == -1_isize {
            return None;
        }
        let mut info: CONSOLE_FONT_INFOEX = std::mem::zeroed();
        info.cb_size = std::mem::size_of::<CONSOLE_FONT_INFOEX>() as u32;
        if GetCurrentConsoleFontEx(handle, 0, &mut info) != 0 {
            let w = info.font_size.x as u16;
            let h = info.font_size.y as u16;
            // conpty often returns 8×16 (raster font default) — skip unrealistic values
            if w >= 6 && h >= 10 && !(w == 8 && h == 16) {
                return Some((w, h));
            }
        }
    }
    None
}

#[cfg(not(windows))]
pub(super) fn get_console_font_size() -> Option<(u16, u16)> {
    None
}

/// Estimate cell pixel size from system DPI (Windows only).
/// Assumes Cascadia Code at default Windows Terminal font size (~12pt).
/// Conservative baseline: 8×16 at 96 DPI. Scales linearly with DPI.
#[cfg(windows)]
pub(super) fn estimate_cell_from_dpi() -> Option<(u16, u16)> {
    extern "system" {
        fn GetDpiForSystem() -> u32;
    }
    // DPI awareness is set in main() — GetDpiForSystem() returns real DPI.
    let mut dpi = unsafe { GetDpiForSystem() };
    eprintln!("[INFO] GetDpiForSystem: {} ({}%)", dpi, dpi * 100 / 96);

    // If DPI reports 96 (might be virtualized), cross-check with registry
    if dpi == 96 {
        if let Some(reg_dpi) = read_dpi_from_registry() {
            if reg_dpi > 96 {
                eprintln!(
                    "[INFO] Registry DPI override: {} ({}%)",
                    reg_dpi,
                    reg_dpi * 100 / 96
                );
                dpi = reg_dpi;
            }
        }
    }

    if dpi == 0 {
        return None;
    }
    let scale = dpi as f32 / 96.0;
    // Cascadia Mono 12pt baseline at 96 DPI: ~9.6w × 20h (empirical)
    let cell_w = (9.6 * scale) as u16;
    let cell_h = (20.0 * scale) as u16;
    Some((cell_w.max(6), cell_h.max(10)))
}

/// Read DPI from Windows registry — works even when API is DPI-virtualized.
#[cfg(windows)]
fn read_dpi_from_registry() -> Option<u32> {
    extern "system" {
        fn RegOpenKeyExA(
            key: isize,
            sub: *const u8,
            opts: u32,
            access: u32,
            result: *mut isize,
        ) -> i32;
        fn RegQueryValueExA(
            key: isize,
            name: *const u8,
            reserved: *mut u32,
            typ: *mut u32,
            data: *mut u8,
            size: *mut u32,
        ) -> i32;
        fn RegCloseKey(key: isize) -> i32;
    }
    unsafe {
        let mut hkey: isize = 0;
        // HKEY_CURRENT_USER = 0x80000001
        let sub = b"Control Panel\\Desktop\0";
        if RegOpenKeyExA(
            0x8000_0001_u32 as isize,
            sub.as_ptr(),
            0,
            0x20019,
            &mut hkey,
        ) != 0
        {
            return None;
        }
        let name = b"LogPixels\0";
        let mut data: u32 = 0;
        let mut size: u32 = 4;
        let mut typ: u32 = 0;
        let result = RegQueryValueExA(
            hkey,
            name.as_ptr(),
            std::ptr::null_mut(),
            &mut typ,
            &mut data as *mut u32 as *mut u8,
            &mut size,
        );
        RegCloseKey(hkey);
        if result == 0 && typ == 4 && data >= 96 {
            Some(data)
        } else {
            None
        }
    }
}

#[cfg(not(windows))]
pub(super) fn estimate_cell_from_dpi() -> Option<(u16, u16)> {
    None
}

/// Query terminal text area pixel size via VT escape sequence CSI 14 t.
/// Must be called AFTER entering raw mode (VT input processing required).
/// Returns (pixel_width, pixel_height) if the terminal supports this query.
/// Works on Windows Terminal (conpty pipe), xterm, foot, and other modern terminals.
#[cfg(windows)]
pub(super) fn query_terminal_pixels_vt() -> Option<(u32, u32)> {
    use std::io::Write;

    extern "system" {
        fn GetStdHandle(n: u32) -> isize;
        fn PeekNamedPipe(
            h: isize,
            buf: *mut u8,
            sz: u32,
            read: *mut u32,
            avail: *mut u32,
            left: *mut u32,
        ) -> i32;
        fn ReadFile(h: isize, buf: *mut u8, sz: u32, read: *mut u32, ovl: *mut u8) -> i32;
    }

    // Send CSI 14 t (report text area pixel size)
    {
        let mut stdout = std::io::stdout().lock();
        stdout.write_all(b"\x1b[14t").ok()?;
        stdout.flush().ok()?;
    }

    let handle = unsafe { GetStdHandle(0xFFFF_FFF6) }; // STD_INPUT_HANDLE
    if handle == 0 || handle == -1isize {
        return None;
    }

    let mut buf = [0u8; 64];
    let mut total = 0usize;
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(200);

    while std::time::Instant::now() < deadline && total < 60 {
        let mut avail: u32 = 0;
        let peek = unsafe {
            PeekNamedPipe(
                handle,
                std::ptr::null_mut(),
                0,
                std::ptr::null_mut(),
                &mut avail,
                std::ptr::null_mut(),
            )
        };
        if peek == 0 || avail == 0 {
            std::thread::sleep(std::time::Duration::from_millis(10));
            continue;
        }
        let to_read = avail.min((60 - total) as u32);
        let mut n: u32 = 0;
        let ok = unsafe {
            ReadFile(
                handle,
                buf[total..].as_mut_ptr(),
                to_read,
                &mut n,
                std::ptr::null_mut(),
            )
        };
        if ok == 0 || n == 0 {
            break;
        }
        total += n as usize;
        if buf[..total].contains(&b't') {
            break;
        }
    }

    if total == 0 {
        eprintln!("[INFO] VT pixel query: no response (terminal may not support CSI 14 t)");
        return None;
    }

    let response = String::from_utf8_lossy(&buf[..total]);
    eprintln!("[INFO] VT pixel query ({} bytes): {:?}", total, response);

    // Parse: ESC [ 4 ; HEIGHT ; WIDTH t
    let s = response.as_ref();
    if let Some(start) = s.find("\x1b[4;") {
        let rest = &s[start + 4..];
        if let Some(end) = rest.find('t') {
            let params = &rest[..end];
            let parts: Vec<&str> = params.split(';').collect();
            if parts.len() == 2 {
                let h: u32 = parts[0].parse().ok()?;
                let w: u32 = parts[1].parse().ok()?;
                if w >= 100 && h >= 50 {
                    return Some((w, h));
                }
            }
        }
    }

    None
}

#[cfg(not(windows))]
pub(super) fn query_terminal_pixels_vt() -> Option<(u32, u32)> {
    // On Unix, crossterm window_size() usually works — handled in detection chain
    None
}

/// Redirect stderr (fd 2) to a log file so eprintln! doesn't corrupt the TUI.
/// Uses CRT-level _dup2 on Windows (SetStdHandle alone doesn't affect Rust's stderr).
pub(super) fn redirect_stderr_to_file() {
    #[cfg(windows)]
    {
        use std::fs::OpenOptions;
        use std::os::windows::io::IntoRawHandle;
        // Use %TEMP% (always exists on Windows) instead of hardcoded C:/tmp/
        let log_path = std::env::var("BALATRO_LOG")
            .or_else(|_| {
                std::env::var("TEMP")
                    .or_else(|_| std::env::var("TMP"))
                    .map(|t| format!("{}/balatro-runtime.log", t))
            })
            .unwrap_or_else(|_| "balatro_tui.log".to_string());
        if let Some(parent) = std::path::Path::new(&log_path).parent() {
            std::fs::create_dir_all(parent).ok();
        }
        if let Ok(file) = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&log_path)
        {
            let handle = file.into_raw_handle();
            unsafe {
                extern "C" {
                    fn _open_osfhandle(osfhandle: isize, flags: i32) -> i32;
                    fn _dup2(fd1: i32, fd2: i32) -> i32;
                }
                let crt_fd = _open_osfhandle(handle as isize, 1); // 1 = _O_WRONLY
                if crt_fd >= 0 {
                    _dup2(crt_fd, 2); // Redirect CRT fd 2 (stderr) to log file
                }
            }
        }
    }
    #[cfg(not(windows))]
    {
        use std::fs::OpenOptions;
        use std::os::unix::io::IntoRawFd;
        let log_path =
            std::env::var("BALATRO_LOG").unwrap_or_else(|_| "/tmp/balatro-runtime.log".to_owned());
        if let Some(parent) = std::path::Path::new(&log_path).parent() {
            std::fs::create_dir_all(parent).ok();
        }
        if let Ok(file) = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(log_path)
        {
            let fd = file.into_raw_fd();
            extern "C" {
                fn dup2(oldfd: i32, newfd: i32) -> i32;
            }
            unsafe {
                dup2(fd, 2);
            }
        }
    }
}

/// Fast resize detection: queries crossterm terminal size (cols/rows).
/// Stores packed (cols<<16|rows) into pending atomic. Zero mutex locks.
/// Called every 15 frames (~4 Hz).
pub(super) fn check_window_resize_detect(state: &SharedState) {
    use std::sync::atomic::Ordering;

    let (cols, rows) = match crossterm::terminal::size() {
        Ok(s) if s.0 >= 10 && s.1 >= 5 => s,
        _ => return,
    };

    let packed = ((cols as u64) << 16) | (rows as u64);
    let prev = state.pending_resize_dims.load(Ordering::Relaxed);

    if packed != prev {
        state.pending_resize_dims.store(packed, Ordering::Relaxed);
        let now_ms = state.start_time.elapsed().as_millis() as u64;
        state
            .pending_resize_changed_ms
            .store(now_ms, Ordering::Relaxed);
    }
}

/// Find the WT window hosting our process by walking the process tree.
/// Collects ALL ancestor PIDs, then finds the largest visible window among them.
#[cfg(windows)]
pub(super) fn find_own_wt_window() -> isize {
    #[repr(C)]
    struct PROCESSENTRY32 {
        dw_size: u32,
        cnt_usage: u32,
        th32_process_id: u32,
        th32_default_heap_id: usize,
        th32_module_id: u32,
        cnt_threads: u32,
        th32_parent_process_id: u32,
        pc_pri_class_base: i32,
        dw_flags: u32,
        sz_exe_file: [u8; 260],
    }
    #[repr(C)]
    struct RECT {
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
    }
    extern "system" {
        fn GetCurrentProcessId() -> u32;
        fn CreateToolhelp32Snapshot(flags: u32, pid: u32) -> isize;
        fn Process32First(snap: isize, entry: *mut PROCESSENTRY32) -> i32;
        fn Process32Next(snap: isize, entry: *mut PROCESSENTRY32) -> i32;
        fn CloseHandle(h: isize) -> i32;
        fn EnumWindows(cb: unsafe extern "system" fn(isize, isize) -> i32, lp: isize) -> i32;
        fn IsWindowVisible(hwnd: isize) -> i32;
        fn GetWindowThreadProcessId(hwnd: isize, pid: *mut u32) -> u32;
        fn GetClientRect(hwnd: isize, rect: *mut RECT) -> i32;
        fn SetThreadDpiAwarenessContext(value: isize) -> isize;
    }

    // Use thread-local storage instead of static mut for safety
    use std::cell::RefCell;
    thread_local! {
        static ANCESTOR_PIDS: RefCell<Vec<u32>> = RefCell::new(Vec::new());
        static BEST: RefCell<(isize, u64)> = RefCell::new((0, 0));
    }

    unsafe {
        let my_pid = GetCurrentProcessId();
        let snap = CreateToolhelp32Snapshot(2, 0); // TH32CS_SNAPPROCESS
        if snap == -1 {
            return 0;
        }

        // Build PID→parent map
        let mut pid_parent: std::collections::HashMap<u32, u32> = std::collections::HashMap::new();
        let mut entry: PROCESSENTRY32 = std::mem::zeroed();
        entry.dw_size = std::mem::size_of::<PROCESSENTRY32>() as u32;
        if Process32First(snap, &mut entry) != 0 {
            loop {
                pid_parent.insert(entry.th32_process_id, entry.th32_parent_process_id);
                if Process32Next(snap, &mut entry) == 0 {
                    break;
                }
            }
        }
        CloseHandle(snap);

        // Collect ALL ancestor PIDs (up to 8 hops)
        let mut ancestors = Vec::with_capacity(8);
        let mut cur = my_pid;
        for _ in 0..8 {
            if let Some(&parent) = pid_parent.get(&cur) {
                if parent == 0 || parent == cur {
                    break;
                }
                ancestors.push(parent);
                cur = parent;
            } else {
                break;
            }
        }

        if ancestors.is_empty() {
            return 0;
        }

        // Store ancestors in thread-local for the callback
        ANCESTOR_PIDS.with(|a| *a.borrow_mut() = ancestors.clone());
        BEST.with(|b| *b.borrow_mut() = (0, 0));

        unsafe extern "system" fn enum_cb(hwnd: isize, _lp: isize) -> i32 {
            if IsWindowVisible(hwnd) != 0 {
                let mut pid = 0u32;
                GetWindowThreadProcessId(hwnd, &mut pid);
                let is_ancestor = ANCESTOR_PIDS.with(|a| a.borrow().contains(&pid));
                if is_ancestor {
                    let mut r = RECT {
                        left: 0,
                        top: 0,
                        right: 0,
                        bottom: 0,
                    };
                    if GetClientRect(hwnd, &mut r) != 0 {
                        let area = (r.right - r.left) as u64 * (r.bottom - r.top) as u64;
                        BEST.with(|b| {
                            let mut best = b.borrow_mut();
                            if area > best.1 {
                                *best = (hwnd, area);
                            }
                        });
                    }
                }
            }
            1 // continue
        }

        let old_ctx = SetThreadDpiAwarenessContext(-4);
        EnumWindows(enum_cb, 0);
        if old_ctx != 0 {
            SetThreadDpiAwarenessContext(old_ctx);
        }

        let (best_hwnd, best_area) = BEST.with(|b| *b.borrow());
        eprintln!(
            "[HWND-DETECT] my_pid={} ancestors={:?} best_hwnd={} best_area={}",
            my_pid, ancestors, best_hwnd, best_area
        );

        best_hwnd
    }
}

/// Query actual terminal pixel area from our own WT window.
/// Finds the window by walking the process tree to the ancestor WT process.
/// Returns (text_width_px, text_height_px) or None.
#[cfg(windows)]
pub(super) fn query_terminal_pixel_area_own(tab_bar_h: u32) -> Option<(u32, u32)> {
    #[repr(C)]
    struct RECT {
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
    }
    extern "system" {
        fn GetClientRect(hwnd: isize, rect: *mut RECT) -> i32;
        fn SetThreadDpiAwarenessContext(value: isize) -> isize;
    }
    let hwnd = find_own_wt_window();
    if hwnd == 0 {
        return None;
    }
    unsafe {
        let old_ctx = SetThreadDpiAwarenessContext(-4);
        let mut rect = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        let ok = GetClientRect(hwnd, &mut rect);
        if old_ctx != 0 {
            SetThreadDpiAwarenessContext(old_ctx);
        }
        if ok == 0 {
            return None;
        }
        let client_w = (rect.right - rect.left) as u32;
        let client_h = (rect.bottom - rect.top) as u32;
        if client_w < 100 || client_h < 50 {
            return None;
        }
        let text_h = client_h.saturating_sub(tab_bar_h);
        if text_h < 50 {
            return None;
        }
        Some((client_w, text_h))
    }
}

#[cfg(not(windows))]
pub(super) fn query_terminal_pixel_area_own(_tab_bar_h: u32) -> Option<(u32, u32)> {
    None
}

/// Debounced resize application: only fires after size is stable for 150ms.
/// Called every frame but does zero work unless a pending resize exists.
const RESIZE_DEBOUNCE_MS: u64 = 150;

pub(super) fn apply_pending_resize(state: &Arc<SharedState>) -> bool {
    use std::sync::atomic::Ordering;

    let packed = state.pending_resize_dims.load(Ordering::Relaxed);
    if packed == 0 {
        return false;
    }

    // Unpack cols/rows from detection phase
    let cols = (packed >> 16) as u16;
    let rows = (packed & 0xFFFF) as u16;

    // Debounce: only apply after size is stable for RESIZE_DEBOUNCE_MS
    let changed_ms = state.pending_resize_changed_ms.load(Ordering::Relaxed);
    let now_ms = state.start_time.elapsed().as_millis() as u64;
    if now_ms.saturating_sub(changed_ms) < RESIZE_DEBOUNCE_MS {
        return false;
    }

    // WT VT340 Sixel: target = cols × 10, rows × 20.
    let target_w = cols as u32 * 10;
    let target_h = rows as u32 * 20;
    let cur_tw = *state.sixel_text_w.lock();
    let cur_th = *state.sixel_text_h.lock();
    if target_w == cur_tw && target_h == cur_th {
        // Same size — no resize needed. Store packed so detect doesn't re-trigger.
        state.pending_resize_dims.store(packed, Ordering::Relaxed);
        return false;
    }

    // Apply the resize.
    state.pending_resize_dims.store(packed, Ordering::Relaxed);

    *state.sixel_text_w.lock() = target_w;
    *state.sixel_text_h.lock() = target_h;

    // Auto-scale canvas: target ≤250K pixels for smooth FPS with CPU shaders.
    // Float-based scaling for optimal quality at budget.
    let pixel_budget: u64 = std::env::var("TUI_PIXELBUDGET")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(250_000);
    let actual_pixels = target_w as u64 * target_h as u64;
    let (new_w, new_h) = if actual_pixels <= pixel_budget {
        (target_w, target_h)
    } else {
        let scale = (actual_pixels as f64 / pixel_budget as f64).sqrt();
        (
            ((target_w as f64 / scale).round() as u32)
                .max(320)
                .min(target_w),
            ((target_h as f64 / scale).round() as u32)
                .max(180)
                .min(target_h),
        )
    };
    *state.canvas_width.lock() = new_w;
    *state.canvas_height.lock() = new_h;
    state.flush_render_jobs();
    state.pixel_buffer.lock().resize(new_w, new_h);

    *state.terminal_cols.lock() = cols;
    *state.terminal_rows.lock() = rows;

    eprintln!(
        "[RESIZE-DEBOUNCED] cols={} rows={} sixel={}x{}, canvas={}x{}",
        cols, rows, target_w, target_h, new_w, new_h
    );

    state
        .event_queue
        .lock()
        .push_back(love_api::state::LoveEvent::Resize { w: new_w, h: new_h });

    true
}
