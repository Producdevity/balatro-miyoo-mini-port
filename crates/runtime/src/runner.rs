// Derived from balatro-port-tui (Apache-2.0). Modified by Producdevity; see NOTICE.

use anyhow::Result;
use crossterm::{
    execute,
    terminal::{
        disable_raw_mode, enable_raw_mode, window_size, EnterAlternateScreen, LeaveAlternateScreen,
    },
};
use mlua::prelude::*;
use ratatui::{
    backend::{Backend, CrosstermBackend, TestBackend},
    Terminal,
};
use std::sync::Arc;
use std::time::{Duration, Instant};

use love_api::state::SharedState;
use love_api::LoveRuntime;
use sprite_to_text::renderer::{encode_sixel, OctantWidget, PixelWidget};

const TARGET_FPS: f64 = 60.0;

/// Rendering mode for terminal output.
#[derive(Clone, Copy, PartialEq)]
enum RenderMode {
    /// Octant characters (2×4 sub-pixels per cell) — default, best Unicode mode
    Octant,
    /// Half-block ▀ (1×2 per cell) — fallback for terminals without octant font support
    HalfBlock,
    /// Sixel graphics protocol — pixel-level rendering, requires terminal support
    Sixel,
    /// Direct Linux framebuffer output used by the handheld build.
    Framebuffer,
    /// No presentation. Used for deterministic host-side profiling and tests.
    Headless,
}

mod terminal;
use terminal::*;

pub fn run(game_path: &std::path::Path) -> Result<()> {
    if std::env::var("BALATRO_PLATFORM").as_deref() == Ok("miyoo") {
        let backend = TestBackend::new(160, 60);
        let mut terminal = Terminal::new(backend)?;
        return run_inner_with_vt(&mut terminal, game_path, None);
    }

    // NOTE: stderr is NOT redirected yet — sysinfo prints go to the real terminal.
    // redirect_stderr_to_file() is called later, just before the game loop starts.

    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen)?;

    // Query terminal pixel dimensions via CSI 14 t BEFORE EnableMouseCapture.
    // Mouse events would flood stdin and mask the VT response.
    let early_vt_pixels = query_terminal_pixels_vt();

    execute!(std::io::stdout(), crossterm::event::EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.hide_cursor()?;

    let result = run_inner_with_vt(&mut terminal, game_path, early_vt_pixels);

    // Cleanup terminal (always, even on error)
    terminal.show_cursor().ok();
    execute!(
        std::io::stdout(),
        crossterm::event::DisableMouseCapture,
        LeaveAlternateScreen
    )
    .ok();
    disable_raw_mode().ok();

    result
}

fn run_inner_with_vt<B: Backend>(
    terminal: &mut Terminal<B>,
    game_path: &std::path::Path,
    _early_vt_pixels: Option<(u32, u32)>,
) -> Result<()> {
    // Internal canvas resolution — proportional to terminal size.
    // Each terminal cell = 1 column × 2 half-rows (via half-block ▀).
    // Scale factor K means K canvas pixels per half-cell in each dimension.
    // K=2 gives good text readability (each game char ≈ 3-4 terminal cols).
    // K=3 gives smoother graphics but smaller text.
    // Canvas is capped at 800×600 for performance.
    let mut size = terminal.size()?;
    // Allow overriding terminal size for headless testing
    if let Ok(v) = std::env::var("TUI_COLS") {
        size.width = v.parse().unwrap_or(size.width);
    }
    if let Ok(v) = std::env::var("TUI_ROWS") {
        size.height = v.parse().unwrap_or(size.height);
    }
    let scale: u32 = std::env::var("TUI_SCALE")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(2);
    let render_mode = match std::env::var("TUI_RENDER").as_deref() {
        Ok("octant") => RenderMode::Octant,
        Ok("halfblock") => RenderMode::HalfBlock,
        Ok("framebuffer") => RenderMode::Framebuffer,
        Ok("headless") => RenderMode::Headless,
        _ => RenderMode::Sixel,
    };

    // Canvas resolution depends on rendering mode:
    //   Octant:    cols*2 × rows*4 (1:1 pixel mapping to sub-pixels, cap 800×600)
    //   HalfBlock: cols*scale × rows*2*scale (cap 800×600)
    //   Sixel:     cols*cell_px_w × rows*cell_px_h (native pixel area)
    // Redirect stderr to log file BEFORE detection — detection messages go to log, not terminal.
    redirect_stderr_to_file();

    eprintln!(
        "[runtime] platform={:?}",
        std::env::var("BALATRO_PLATFORM").unwrap_or_else(|_| "terminal".to_owned())
    );
    let canvas_width;
    let canvas_height;
    let mut cell_px_w: u16 = 0;
    let mut cell_px_h: u16 = 0;
    let mut cell_px_w_frac: f32 = 0.0;
    let mut cell_px_h_frac: f32 = 0.0;
    let mut sixel_target_w: u32 = 0;
    let mut sixel_target_h: u32 = 0;
    let mut sixel_cell_w_init: f32 = 0.0;
    let mut detected_hwnd: isize = 0;
    let mut detected_tab_h: u32 = 0;
    match render_mode {
        RenderMode::Octant => {
            canvas_width = (size.width as u32 * 2).min(800);
            canvas_height = (size.height as u32 * 4).min(600);
        }
        RenderMode::HalfBlock => {
            canvas_width = (size.width as u32 * scale).min(800);
            canvas_height = (size.height as u32 * 2 * scale).min(600);
        }
        RenderMode::Framebuffer | RenderMode::Headless => {
            canvas_width = std::env::var("BALATRO_RENDER_WIDTH")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(320);
            canvas_height = std::env::var("BALATRO_RENDER_HEIGHT")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(240);
        }
        RenderMode::Sixel => {
            // Sixel needs pixel dimensions as well as terminal rows and columns.
            // An explicit cell size overrides the terminal's reported dimensions.
            let tui_cell_override: Option<(u16, u16)> =
                std::env::var("TUI_CELL").ok().and_then(|s| {
                    let parts: Vec<&str> = s.split('x').collect();
                    if parts.len() == 2 {
                        Some((parts[0].parse().ok()?, parts[1].parse().ok()?))
                    } else {
                        None
                    }
                });

            // raw_pixel_area: exact (width, text_height) from window detection
            let mut raw_pixel_area: Option<(u32, u32)> = None;

            // Primary: find our own WT window via process tree walk.
            // GetForegroundWindow is unreliable (may detect the launcher's terminal).
            if let Some((_, _, _pw, _ph, hwnd, tab_h)) =
                get_foreground_window_cell_size(size.width, size.height)
            {
                detected_hwnd = hwnd;
                detected_tab_h = tab_h;
            }
            // Process tree detection: finds the actual WT window hosting us.
            if let Some((pw, ph)) = query_terminal_pixel_area_own(if detected_tab_h > 0 {
                detected_tab_h
            } else {
                32
            }) {
                let cw_check = pw / size.width as u32;
                let ch_check = ph / size.height as u32;
                eprintln!(
                    "[INFO] ProcessTree window: {}x{}, cell={}x{}",
                    pw, ph, cw_check, ch_check
                );
                if cw_check >= 6 && cw_check <= 16 && ch_check >= 8 && ch_check <= 32 {
                    raw_pixel_area = Some((pw, ph));
                }
            } else if detected_hwnd != 0 {
                // Fallback to ForegroundWindow if process tree failed
                if let Some((_, _, pw, ph, _, _)) =
                    get_foreground_window_cell_size(size.width, size.height)
                {
                    let cw_check = pw / size.width as u32;
                    let ch_check = ph / size.height as u32;
                    if cw_check >= 6 && cw_check <= 16 && ch_check >= 8 && ch_check <= 32 {
                        raw_pixel_area = Some((pw, ph));
                    }
                }
            }

            let (cell_w, cell_h): (u16, u16) = if let Some(v) = tui_cell_override {
                v
            } else if raw_pixel_area.is_some() {
                let (pw, ph) = raw_pixel_area.unwrap();
                (
                    (pw / size.width as u32) as u16,
                    (ph / size.height as u32) as u16,
                )
            } else if let Some((pw, ph)) = query_terminal_pixels_vt() {
                raw_pixel_area = Some((pw, ph));
                let cw = (pw / size.width as u32) as u16;
                let ch = (ph / size.height as u32) as u16;
                (cw.max(4), ch.max(8))
            } else {
                None.or_else(|| {
                    get_console_pixel_size()
                        .filter(|&(w, h)| w > 0 && h > 0)
                        .map(|(w, h)| {
                            raw_pixel_area = Some((w, h));
                            (w as u16 / size.width, h as u16 / size.height)
                        })
                        .filter(|&(cw, ch)| cw >= 4 && ch >= 8)
                })
                .or_else(get_console_font_size)
                .or_else(|| {
                    window_size()
                        .ok()
                        .filter(|ws| ws.columns > 0 && ws.rows > 0)
                        .map(|ws| {
                            raw_pixel_area = Some((ws.width as u32, ws.height as u32));
                            (ws.width / ws.columns, ws.height / ws.rows)
                        })
                        .filter(|&(cw, ch)| cw >= 4 && ch >= 8)
                })
                .or_else(estimate_cell_from_dpi)
                .unwrap_or((9, 20))
            };
            cell_px_w = cell_w;
            cell_px_h = cell_h;

            // Compute fractional cell pixel sizes for accurate resize calculations.
            // Integer cell sizes (9, 19) lose ~3% per axis when multiplied back.
            // Fractional (9.275, 19.83) preserve exact pixel area on resize.
            if let Some((pw, ph)) = raw_pixel_area {
                cell_px_w_frac = pw as f32 / size.width as f32;
                // Divide by FULL rows (not rows-1). The safety margin row is subtracted
                // separately via eff_rows in the resize handler.
                cell_px_h_frac = ph as f32 / size.height as f32;
                eprintln!(
                    "[INFO] Fractional cell: {:.3}x{:.3}px",
                    cell_px_w_frac, cell_px_h_frac
                );
            } else {
                cell_px_w_frac = cell_w as f32;
                cell_px_h_frac = cell_h as f32;
            }

            // WT's Sixel renderer emulates VT340 with a fixed virtual cell of 10×20 pixels.
            // Sixel pixel coordinates are mapped to terminal cells using this virtual size,
            // then displayed at the actual font cell dimensions. To fill N columns, the
            // Sixel image must be N×10 pixels wide (not N×actual_cell_w).
            // See: https://github.com/microsoft/terminal/pull/17421
            const VT340_CELL_W: u32 = 10;
            const VT340_CELL_H: u32 = 20;
            let stw = size.width as u32 * VT340_CELL_W;
            let sth = size.height as u32 * VT340_CELL_H;
            sixel_target_w = stw;
            sixel_target_h = sth;
            sixel_cell_w_init = VT340_CELL_W as f32;

            // Auto-scale canvas based on Sixel target pixel budget.
            let init_pixel_budget: u64 = std::env::var("TUI_PIXELBUDGET")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(250_000);
            let init_actual = stw as u64 * sth as u64;
            if let Some(manual_scale) = std::env::var("TUI_SCALE")
                .ok()
                .and_then(|s| s.parse::<u32>().ok())
            {
                canvas_width = (stw / manual_scale).max(320);
                canvas_height = (sth / manual_scale).max(180);
            } else if init_actual <= init_pixel_budget {
                canvas_width = stw;
                canvas_height = sth;
            } else {
                let scale = (init_actual as f64 / init_pixel_budget as f64).sqrt();
                canvas_width = ((stw as f64 / scale).round() as u32).max(320).min(stw);
                canvas_height = ((sth as f64 / scale).round() as u32).max(180).min(sth);
            }
            eprintln!(
                "[INFO] Sixel: vt340_cell={}x{}, term={}x{}, sixel_target={}x{}, canvas={}x{}",
                VT340_CELL_W,
                VT340_CELL_H,
                size.width,
                size.height,
                stw,
                sth,
                canvas_width,
                canvas_height
            );
        }
    }

    let mode_name = match render_mode {
        RenderMode::Octant => "octant",
        RenderMode::HalfBlock => "halfblock",
        RenderMode::Sixel => "sixel",
        RenderMode::Framebuffer => "framebuffer",
        RenderMode::Headless => "headless",
    };
    eprintln!(
        "[INFO] Terminal: {}x{}, Scale: {}x, Render: {}, Internal canvas: {}x{}",
        size.width, size.height, scale, mode_name, canvas_width, canvas_height
    );

    let render_mode_id: u8 = match render_mode {
        RenderMode::Octant => 0,
        RenderMode::HalfBlock => 1,
        RenderMode::Sixel => 2,
        RenderMode::Framebuffer => 3,
        RenderMode::Headless => 4,
    };
    let state = Arc::new(SharedState::new(
        game_path,
        canvas_width,
        canvas_height,
        size.width,
        size.height,
        scale,
        render_mode_id,
        cell_px_w,
        cell_px_h,
        cell_px_w_frac,
        cell_px_h_frac,
        detected_hwnd,
        detected_tab_h,
        sixel_cell_w_init,
    )?);

    // Set Sixel output target dimensions (exact terminal pixel area)
    if sixel_target_w > 0 && sixel_target_h > 0 {
        *state.sixel_text_w.lock() = sixel_target_w;
        *state.sixel_text_h.lock() = sixel_target_h;
    }

    let runtime = LoveRuntime::new(Arc::clone(&state))?;
    let lua = &runtime.lua;

    // Broad JIT tracing exceeded the ARM memory budget in testing.
    // Interpreter mode is the device default; scoped JIT tests opt in below.
    lua.load("local ok, loaded_jit = pcall(require, 'jit'); if ok then loaded_jit.off() end")
        .exec()
        .map_err(|e| anyhow::anyhow!("disable LuaJIT traces: {e}"))?;

    // Lua 5.1 otherwise starts every process with the same random seed.
    lua.load("math.randomseed(os.time() + os.clock() * 1000000)")
        .exec()
        .map_err(|e| anyhow::anyhow!("initial RNG seed: {}", e))?;

    // Host builds use Lua 5.1, whose integer seed conversion loses Balatro's
    // fractional seeds. Scale only small values so timestamp seeds stay in range.
    lua.load(
        r#"
        local _orig_randomseed = math.randomseed
        math.randomseed = function(seed)
            if type(seed) == 'number' and seed >= -1 and seed <= 1 then
                -- Scale small float seeds to large integers to preserve entropy
                seed = math.floor(seed * 2147483647)  -- 2^31 - 1
            end
            return _orig_randomseed(seed)
        end
    "#,
    )
    .exec()
    .map_err(|e| anyhow::anyhow!("math.randomseed patch: {}", e))?;

    // Balatro's seeded generators need independent state, not math.random's state.
    lua.load(include_str!("compat/random.lua"))
        .exec()
        .map_err(|e| anyhow::anyhow!("newRandomGenerator patch: {}", e))?;

    lua.load(include_str!("compat/noise.lua"))
        .exec()
        .map_err(|e| anyhow::anyhow!("Perlin noise patch: {}", e))?;

    {
        let source = state.game_source.lock();
        if let Ok(conf_src) = source.read_file("conf.lua") {
            drop(source);
            if let Ok(code) = std::str::from_utf8(&conf_src) {
                if let Err(e) = lua.load(code).set_name("@conf.lua").exec() {
                    eprintln!("[WARN] conf.lua: {}", e);
                }
            }
        }
    }

    // Preserve the desktop CRT's dark teal background without its post-process.
    lua.load(
        r#"
        local _orig_clear = love.graphics.clear
        love.graphics.clear = function(r, g, b, a)
            if type(r) == 'number' and r == 0 and g == 0 and b == 0 then
                return _orig_clear(0.216, 0.259, 0.267, a or 1)
            elseif type(r) == 'table' and r[1] == 0 and r[2] == 0 and r[3] == 0 then
                r[1], r[2], r[3] = 0.216, 0.259, 0.267
                return _orig_clear(r)
            end
            return _orig_clear(r, g, b, a)
        end
    "#,
    )
    .exec()
    .ok();

    // Select 1x atlases before main.lua builds their paths. The renderer's quad
    // coordinates use the same scale; changing it afterward misaligns sprites.
    lua.load(include_str!("compat/game_hooks.lua")).exec().ok();

    let main_src = {
        let source = state.game_source.lock();
        source.read_file("main.lua")?
    };
    let main_code = std::str::from_utf8(&main_src)?;
    lua.load(main_code)
        .set_name("@main.lua")
        .exec()
        .map_err(|e| anyhow::anyhow!("main.lua error: {}", e))?;

    if std::env::var("BALATRO_PLATFORM").as_deref() == Ok("miyoo") {
        lua.load(love_api::state::MIYOO_SMALL_SCREEN_PATCH)
            .set_name("@miyoo/small_screen.lua")
            .exec()
            .map_err(|error| anyhow::anyhow!("small-screen layout: {error}"))?;
        lua.load(love_api::state::MIYOO_PAYOUT_PATCH)
            .set_name("@miyoo/payout.lua")
            .exec()
            .map_err(|error| anyhow::anyhow!("payout layout: {error}"))?;
        eprintln!("[layout] 4:3 small-screen layout installed");
    }

    // Balatro disables its keyboard-to-gamepad bridge in release builds.
    // Install the handheld mapping after main.lua has created the callbacks.
    crate::miyoo_input::install(&lua).map_err(|error| anyhow::anyhow!("input mapping: {error}"))?;

    lua.load(include_str!("compat/tutorial.lua")).exec().ok();

    // Finish card flips immediately because the reduced-motion renderer does
    // not use the width-collapse transition that normally swaps the card face.
    lua.load(include_str!("compat/reduced_motion.lua"))
        .exec()
        .ok();

    // The source loader disables per-object shadows at construction time.
    // Keep the global setting off as well because saves may restore it.
    lua.load(
        r#"
        if G and G.SETTINGS and G.SETTINGS.GRAPHICS then
            G.SETTINGS.GRAPHICS.shadows = 'Off'
        end
    "#,
    )
    .exec()
    .map_err(|e| anyhow::anyhow!("disable shadows: {}", e))?;

    // Keep Lua I/O out of the terminal render stream.
    lua.load(
        r#"
        io.output(io.stderr)
        -- Replace io.stdout with io.stderr so io.stdout:write() goes to log file.
        -- io.stdout is a FILE* userdata in Lua 5.1, can't set fields on it.
        io.stdout = io.stderr
    "#,
    )
    .exec()
    .map_err(|e| anyhow::anyhow!("io redirect patch: {}", e))?;

    // Cursor-based seeds repeat in terminal mode, where the cursor stays at (0,0).
    lua.load(
        r#"
        do
            local _orig_generate_starting_seed = generate_starting_seed
            if _orig_generate_starting_seed then
                generate_starting_seed = function()
                    return random_string(8, os.time() + os.clock() * 1000000)
                end
            end
        end
    "#,
    )
    .exec()
    .ok();

    let love_table: LuaTable = lua.globals().get("love")?;

    if let Ok(run_fn) = love_table.get::<LuaFunction>("run") {
        // Balatro path: love.run() returns a frame closure
        eprintln!("[INFO] Calling love.run()...");
        let frame_fn_value: LuaValue = run_fn.call(())?;

        if let LuaValue::Function(frame_fn) = &frame_fn_value {
            if let Some(enabled) = crate::lua_jit::configure(lua, frame_fn)
                .map_err(|error| anyhow::anyhow!("configure selective LuaJIT: {error}"))?
            {
                eprintln!("[lua] selective JIT scope enabled: {enabled}");
            }
        }

        let lua_backend: String = lua
            .load(
                r#"
                local ok, loaded_jit = pcall(require, 'jit')
                if not ok or not loaded_jit then return _VERSION .. ' interpreter' end
                local status = {loaded_jit.status()}
                return string.format('%s %s %s', loaded_jit.version,
                    loaded_jit.arch, status[1] and 'enabled' or 'disabled')
            "#,
            )
            .eval()
            .unwrap_or_else(|error| format!("unknown ({error})"));
        eprintln!("[lua] {lua_backend}");

        lua.load(include_str!("balatro_save.lua"))
            .set_name("@balatro_save.lua")
            .exec()
            .map_err(|error| anyhow::anyhow!("install save bridge: {error}"))?;
        eprintln!("[filesystem] synchronous save bridge installed");

        // After love.run() initializes the game, disable shadows globally.
        // G.SETTINGS is now populated by the game's init code.
        lua.load(
            r#"
            if G and G.SETTINGS and G.SETTINGS.GRAPHICS then
                G.SETTINGS.GRAPHICS.shadows = 'Off'
            end
        "#,
        )
        .exec()
        .ok();

        // stderr is already redirected to log file (done after canvas detection)

        // In Sixel mode: set background black, clear screen, hide cursor, disable Sixel scrolling
        if render_mode == RenderMode::Sixel {
            use std::io::Write;
            let mut stdout = std::io::stdout().lock();
            // OSC 11: set terminal background to pure black (covers padding/scrollbar areas)
            // ANSI 48;2: set text cell background to black
            // 2J: clear screen with black background
            // H: cursor home, ?25l: hide cursor, ?80h: DECSDM (no-scroll Sixel)
            // Set background black (matches game border), hide cursor, enable DECSDM (no-scroll Sixel).
            let _ = stdout.write_all(
                b"\x1b]11;rgb:00/00/00\x1b\\\x1b[48;2;0;0;0m\x1b[2J\x1b[H\x1b[?25l\x1b[?80h",
            );
            let _ = stdout.flush();
            // Start async writer thread — decouples Sixel encode from stdout write
            start_sixel_writer_thread();
        }

        match frame_fn_value {
            LuaValue::Function(frame_fn) => {
                eprintln!("[INFO] Got frame function, entering main loop");
                run_frame_loop(terminal, lua, &frame_fn, &state, render_mode)?;
            }
            _ => {
                eprintln!("[WARN] love.run() did not return a function, using fallback loop");
                run_fallback_loop(terminal, lua, &state, render_mode)?;
            }
        }
    } else {
        // Standard LÖVE path: use default lifecycle
        eprintln!("[INFO] No love.run(), using default lifecycle");
        run_fallback_loop(terminal, lua, &state, render_mode)?;
    }

    Ok(())
}

/// Main loop for games that define love.run() returning a frame function (Balatro)
fn run_frame_loop<B: Backend>(
    terminal: &mut Terminal<B>,
    lua: &Lua,
    frame_fn: &LuaFunction,
    state: &Arc<SharedState>,
    render_mode: RenderMode,
) -> Result<()> {
    let frame_duration = Duration::from_nanos((1_000_000_000.0 / TARGET_FPS) as u64);
    let outer_clear = std::env::var("BALATRO_OUTER_CLEAR").as_deref() == Ok("1");
    let queued_output = render_mode == RenderMode::Framebuffer
        && std::env::var("BALATRO_FRAME_PIPELINE").as_deref() == Ok("2");
    anyhow::ensure!(
        !queued_output || !outer_clear,
        "frame pipeline cannot use an outer clear"
    );
    eprintln!("[video] game-owned frame clear; outer_clear={outer_clear}");
    #[cfg(all(feature = "copy-trace", target_os = "linux", target_arch = "arm"))]
    let _copy_trace = crate::copy_trace::CopyTrace::start();
    #[cfg(all(feature = "native-sampler", target_os = "linux", target_arch = "arm"))]
    let _sampler = love_api::native_sampler::Sampler::for_thread("main")
        .map_err(|error| eprintln!("[native-sample] unavailable: {error}"))
        .ok()
        .flatten();
    let mut frame_count: u64 = 0;
    let profile_mode = std::env::var("BALATRO_PROFILE").unwrap_or_default();
    let profile_enabled = profile_mode == "1" || profile_mode == "phases";
    let profile_owners = profile_mode == "1";
    let force_reduced_motion = std::env::var("BALATRO_REDUCED_MOTION").as_deref() == Ok("1");
    lua.globals()
        .set("_SVMM_FORCE_REDUCED_MOTION", force_reduced_motion)?;
    if force_reduced_motion {
        eprintln!("[runtime] reduced motion enabled");
    }
    let mut benchmark = crate::benchmark::FrameBenchmark::from_env();
    let mut incremental_gc = crate::incremental_gc::IncrementalGc::from_env();
    let gc_overlap = std::env::var("BALATRO_GC_OVERLAP").as_deref() != Ok("0");
    let lua_sampler = crate::lua_sampler::LuaSampler::from_env(lua);
    eprintln!(
        "[gc] incremental step={} KiB/frame overlap={gc_overlap}",
        incremental_gc.step_kib(),
    );
    let benchmark_game = if benchmark.is_enabled() {
        lua.globals().get::<LuaTable>("G").ok()
    } else {
        None
    };
    if let Some(game) = &benchmark_game {
        let mut state_names = std::collections::BTreeMap::new();
        if let Ok(states) = game.get::<LuaTable>("STATES") {
            for pair in states.pairs::<String, i64>() {
                if let Ok((name, value)) = pair {
                    state_names.entry(value).or_insert(name);
                }
            }
        }
        benchmark.set_state_names(state_names);
    }
    if profile_enabled {
        lua.globals().set("_SVMM_PROFILE_OWNERS", profile_owners)?;
        lua.globals().set("_SVMM_PHASE_PROFILE", true)?;
        lua.load(include_str!("diagnostics/profile.lua")).exec()?;
        lua.globals().set("_SVMM_PROFILE_OWNERS", LuaNil)?;

        if std::env::var("BALATRO_PROFILE_SKIP_HUD").as_deref() == Ok("1") {
            lua.load(
                r#"
                local profiled_uibox_draw = UIBox.draw
                UIBox.draw = function(self, ...)
                    if self == G.HUD or self == G.HUD_blind then
                        self.FRAME.DRAW = G.FRAMES.DRAW
                        return
                    end
                    return profiled_uibox_draw(self, ...)
                end
                "#,
            )
            .exec()?;
            eprintln!("[profile] HUD drawing disabled for upper-bound measurement");
        }
    }

    // Auto-input for testing gameplay progression
    let autoplay = std::env::var("TUI_AUTOPLAY").as_deref() == Ok("1");
    if autoplay {
        lua.globals().set("_TUI_AUTOPLAY", true)?;
        let seed = std::env::var("TUI_AUTOPLAY_SEED").unwrap_or_else(|_| "MIYOO30".to_owned());
        eprintln!("[autoplay] seed={seed}");
        lua.globals().set("_TUI_AUTOPLAY_SEED", seed)?;
        let delay = std::env::var("TUI_AUTOPLAY_DELAY")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(850);
        lua.globals().set("_TUI_AUTOPLAY_DELAY", delay)?;
        let settle_frames = std::env::var("TUI_AUTOPLAY_SETTLE_FRAMES")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(240);
        lua.globals()
            .set("_TUI_AUTOPLAY_SETTLE_FRAMES", settle_frames)?;
        let menu_interval = std::env::var("TUI_AUTOPLAY_MENU_INTERVAL")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(180);
        lua.globals()
            .set("_TUI_AUTOPLAY_MENU_INTERVAL", menu_interval)?;
        let hold_hand = std::env::var("TUI_AUTOPLAY_HOLD_HAND")
            .map(|value| value == "1")
            .unwrap_or(false);
        lua.globals().set("_TUI_AUTOPLAY_HOLD_HAND", hold_hand)?;
        let hold_shop = std::env::var("TUI_AUTOPLAY_HOLD_SHOP")
            .map(|value| value == "1")
            .unwrap_or(false);
        lua.globals().set("_TUI_AUTOPLAY_HOLD_SHOP", hold_shop)?;
        let hold_round_eval = std::env::var("TUI_AUTOPLAY_HOLD_ROUND_EVAL")
            .map(|value| value == "1")
            .unwrap_or(false);
        lua.globals()
            .set("_TUI_AUTOPLAY_HOLD_ROUND_EVAL", hold_round_eval)?;
        let payout_jokers = std::env::var("TUI_AUTOPLAY_PAYOUT_JOKERS")
            .ok()
            .and_then(|value| value.parse::<u8>().ok())
            .unwrap_or(0)
            .min(5);
        lua.globals()
            .set("_TUI_AUTOPLAY_PAYOUT_JOKERS", payout_jokers)?;
        let test_blind_chips = std::env::var("TUI_AUTOPLAY_TEST_BLIND_CHIPS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .filter(|value| *value > 0);
        lua.globals()
            .set("_TUI_AUTOPLAY_TEST_BLIND_CHIPS", test_blind_chips)?;
        let stress_effects = std::env::var("TUI_AUTOPLAY_STRESS_EFFECTS").unwrap_or_default();
        lua.globals().set(
            "_TUI_AUTOPLAY_STRESS_EFFECTS",
            matches!(stress_effects.as_str(), "1" | "negative"),
        )?;
        lua.globals()
            .set("_TUI_AUTOPLAY_NEGATIVE_CARDS", stress_effects == "negative")?;
        eprintln!("[INFO] Autoplay mode enabled");
    }
    let frame_limit = std::env::var("BALATRO_TEST_FRAMES")
        .ok()
        .and_then(|value| value.parse::<u64>().ok());
    let auto_input_fn: Option<LuaFunction> = if autoplay {
        Some(lua.load(include_str!("replays/autoplay.lua")).eval()?)
    } else {
        None
    };

    let controls_test = if frame_limit.is_some() {
        let script = match std::env::var("BALATRO_TEST_CONTROLS").as_deref() {
            Ok("1") => Some(include_str!("playability_test.lua")),
            Ok("rapid-input") => Some(include_str!("rapid_input_test.lua")),
            Ok("evdev-input") => Some(include_str!("evdev_input_test.lua")),
            Ok("music-transitions") => Some(include_str!("music_test.lua")),
            Ok("blind") => Some(include_str!("blind_tooltip_test.lua")),
            Ok("layout") => Some(include_str!("layout_test.lua")),
            Ok("scoring") => Some(include_str!("scoring_layout_test.lua")),
            Ok("collection" | "collection-slow") => Some(include_str!("collection_test.lua")),
            _ => None,
        };
        script
            .map(|source| lua.load(source).eval::<LuaFunction>())
            .transpose()?
    } else {
        None
    };

    let slow_test = controls_test.is_some()
        && matches!(
            std::env::var("BALATRO_TEST_CONTROLS").as_deref(),
            Ok("collection-slow" | "music-transitions")
        );

    // Saves can restore graphics settings after initialization.
    let enforce_settings_fn: LuaFunction = lua
        .load(
            r#"
        return function()
            if G and G.SETTINGS and G.SETTINGS.GRAPHICS then
                G.SETTINGS.GRAPHICS.shadows = 'Off'
            end
            if _SVMM_FORCE_REDUCED_MOTION and G and G.SETTINGS then
                G.SETTINGS.reduced_motion = true
            end
        end
    "#,
        )
        .eval()?;

    let mut queued_present = queued_output.then(crate::queued_present::QueuedPresent::default);
    eprintln!("[video] queued presentation={queued_output}");
    let mut previous_frame_end = Instant::now();

    loop {
        let frame_start = Instant::now();
        frame_count += 1;

        if profile_owners {
            lua.globals()
                .set("_SVMM_PROFILE_SAMPLE", frame_count % 120 == 0)?;
        }

        enforce_settings_fn.call::<()>(()).ok();

        // Reset per-frame canvas tracking so auto-clear fires on first setCanvas
        state.canvases_activated_this_frame.lock().clear();

        // A custom love.run owns clearing. Keep the previous extra clear only
        // as an A/B reference; the default lifecycle still clears before draw.
        if outer_clear {
            state.flush_render_jobs();
            let bg = *state.background_color.lock();
            state.pixel_buffer.lock().clear(bg[0], bg[1], bg[2], bg[3]);
        }

        let t_lua_start = Instant::now();

        if render_mode == RenderMode::Framebuffer {
            crate::platform::poll_input(state)?;
        }

        // Call the frame function
        match frame_fn.call::<LuaValue>(()) {
            Ok(LuaValue::Integer(code)) => {
                eprintln!("[INFO] Game quit with code: {}", code);
                break;
            }
            Ok(LuaValue::Number(code)) => {
                eprintln!("[INFO] Game quit with code: {}", code);
                break;
            }
            Ok(_) => {}
            Err(e) => {
                eprintln!("[ERROR] Frame error: {}", e);
                break;
            }
        }

        let t_lua = t_lua_start.elapsed();
        // Draw jobs own their pixel data. Collect Lua objects while the worker
        // finishes this frame, before waiting for its completed buffer.
        if gc_overlap {
            if let Err(error) = incremental_gc.step(lua) {
                eprintln!("[gc] incremental step failed: {error}");
            }
        }
        let mut t_raster_wait = if queued_output {
            Duration::ZERO
        } else {
            let start = Instant::now();
            state.flush_render_jobs();
            start.elapsed()
        };

        // Auto-input testing (every frame to not miss trigger frames)
        if let Some(auto_input_fn) = &auto_input_fn {
            if let Err(e) = auto_input_fn.call::<()>(frame_count) {
                eprintln!("[AUTO] error: {}", e);
            }
        }

        if let Some(test) = &controls_test {
            let result = test.call::<Option<String>>(frame_count);
            let label = match &result {
                Ok(label) => label.as_deref(),
                Err(_) => Some("failed"),
            };
            if let Some(label) = label {
                let directory = std::path::PathBuf::from(std::env::var("BALATRO_TEST_SNAPSHOT")?);
                let path = directory.with_file_name(format!("controls-{label}.ppm"));
                state.flush_render_jobs();
                state
                    .pixel_buffer
                    .lock()
                    .save_ppm(&path.to_string_lossy())?;
            }
            result?;
            if slow_test && frame_count >= 100 {
                std::thread::sleep(Duration::from_millis(180));
            }
        }

        if !gc_overlap {
            if let Err(error) = incremental_gc.step(lua) {
                eprintln!("[gc] incremental step failed: {error}");
            }
        }

        if *state.should_quit.lock() {
            break;
        }

        let t_crt_start = Instant::now();

        // Apply CRT post-processing (bloom + contrast + vignette + scanlines)
        if !queued_output {
            let crt = *state.crt_params.lock(); // [bloom_fac, crt_intensity]
            let mut pb = state.pixel_buffer.lock();
            pb.apply_crt_effect(crt[0], crt[1]);
        }

        let t_crt = if queued_output {
            Duration::ZERO
        } else {
            t_crt_start.elapsed()
        };

        // Save PPM snapshots at key frames (TUI_SNAPSHOT env var)
        if std::env::var("TUI_SNAPSHOT").is_ok() {
            let save = frame_count % 200 == 0 && frame_count >= 200 && frame_count <= 12000;
            if save {
                state.flush_render_jobs();
                let pb = state.pixel_buffer.lock();
                let path = format!("C:/tmp/balatro_snap_f{}.ppm", frame_count);
                if let Err(e) = pb.save_ppm(&path) {
                    eprintln!("[SNAP] error: {}", e);
                } else {
                    eprintln!("[SNAP] Saved {} ({}x{})", path, pb.width, pb.height);
                }
            }
        }

        // Debounced resize detection for Sixel mode.
        // Detection: zero-lock GetClientRect every 15 frames (~4 Hz).
        // Application: only after size stable for 150ms (avoids lag during maximize animation).
        if render_mode == RenderMode::Sixel {
            if frame_count % 15 == 0 {
                check_window_resize_detect(state);
            }
            apply_pending_resize(state);
        }

        // Render
        let t_present_start = Instant::now();
        let t_present = if let Some(output) = &mut queued_present {
            t_raster_wait = output.submit(state, *state.crt_params.lock())?;
            Duration::ZERO
        } else {
            render_frame(terminal, state, render_mode)?;
            t_present_start.elapsed()
        };

        // FPS tracking
        let frame_end = Instant::now();
        state
            .fps_counter
            .lock()
            .update(frame_end.duration_since(previous_frame_end).as_secs_f32());
        previous_frame_end = frame_end;
        let benchmark_state = benchmark_game
            .as_ref()
            .and_then(|game| game.get::<i64>("STATE").ok());
        benchmark.record(frame_start.elapsed(), benchmark_state);

        if frame_count % 120 == 0 {
            let fps = state.fps_counter.lock().current_fps;
            let gc_stats = incremental_gc.take_stats();
            eprintln!(
                "[FRAME] F={} lua={:.1}ms raster-wait={:.1}ms crt={:.1}ms present={:.1}ms total={:.1}ms FPS={:.1}",
                frame_count,
                t_lua.as_secs_f64() * 1000.0,
                t_raster_wait.as_secs_f64() * 1000.0,
                t_crt.as_secs_f64() * 1000.0,
                t_present.as_secs_f64() * 1000.0,
                frame_start.elapsed().as_secs_f64() * 1000.0,
                fps
            );
            eprintln!(
                "[gc] steps={} cycles={} avg={:.3}ms max={:.3}ms",
                gc_stats.steps,
                gc_stats.cycles,
                gc_stats.total.as_secs_f64() * 1000.0 / gc_stats.steps.max(1) as f64,
                gc_stats.longest.as_secs_f64() * 1000.0
            );
            if let Some(sampler) = &lua_sampler {
                eprintln!("[lua-samples] {}", sampler.take_report(lua));
            }
            if let Some(profile) = love_api::graphics::take_draw_profile() {
                eprintln!(
                    "[graphics] draw={:.1}ms/{} ui_cache={}/{} ui_fill={}/{}/{} ui_scan_angle={}/{}/{}/{} ui_text={} axis={:.1}ms/{} rotated={:.1}ms/{:.1} nearest={:.1}ms/{} linear={:.1}ms/{} box={:.1}ms/{} effect={:.1}ms/{:.1} mask={:.1}ms/{} mask_pixels={} axis_pixels={} clear={:.1}ms/{} rect={:.1}ms/{} line={:.1}ms/{} ellipse={:.1}ms/{} polygon={:.1}ms/{} stencil={:.1}ms/{} deferred={:.1}ms/{} per-frame",
                    profile.draw_ns as f64 / 120_000_000.0,
                    profile.draw_calls / 120,
                    profile.ui_cache_hits / 120,
                    profile.ui_cache_misses / 120,
                    profile.ui_simple_fills / 120,
                    profile.ui_scanline_fills / 120,
                    profile.ui_polygon_lines / 120,
                    profile.ui_scanline_axis / 120,
                    profile.ui_scanline_tiny_rotation / 120,
                    profile.ui_scanline_small_rotation / 120,
                    profile.ui_scanline_large_rotation / 120,
                    profile.ui_text_draws / 120,
                    profile.axis_ns as f64 / 120_000_000.0,
                    profile.axis_calls / 120,
                    profile.rotated_ns as f64 / 120_000_000.0,
                    profile.rotated_calls as f64 / 120.0,
                    profile.nearest_ns as f64 / 120_000_000.0,
                    profile.nearest_calls / 120,
                    profile.linear_ns as f64 / 120_000_000.0,
                    profile.linear_calls / 120,
                    profile.box_ns as f64 / 120_000_000.0,
                    profile.box_calls / 120,
                    profile.effect_ns as f64 / 120_000_000.0,
                    profile.effect_calls as f64 / 120.0,
                    profile.mask_ns as f64 / 120_000_000.0,
                    profile.mask_calls / 120,
                    profile.mask_pixels / 120,
                    profile.axis_pixels / 120,
                    profile.clear_ns as f64 / 120_000_000.0,
                    profile.clear_calls / 120,
                    profile.rect_ns as f64 / 120_000_000.0,
                    profile.rect_calls / 120,
                    profile.line_ns as f64 / 120_000_000.0,
                    profile.line_calls / 120,
                    profile.ellipse_ns as f64 / 120_000_000.0,
                    profile.ellipse_calls / 120,
                    profile.polygon_ns as f64 / 120_000_000.0,
                    profile.polygon_calls / 120,
                    profile.stencil_ns as f64 / 120_000_000.0,
                    profile.stencil_calls / 120,
                    profile.deferred_ns as f64 / 120_000_000.0,
                    profile.deferred_calls / 120
                );
            }
            if profile_enabled {
                let (image_count, image_bytes) = {
                    let images = state.images.lock();
                    let mut seen = std::collections::HashSet::new();
                    (
                        images.len(),
                        images
                            .values()
                            .filter(|image| seen.insert(Arc::as_ptr(image)))
                            .map(|image| image.pixels.capacity())
                            .sum::<usize>(),
                    )
                };
                let (canvas_count, canvas_bytes) = {
                    let canvases = state.canvases.lock();
                    (
                        canvases.len(),
                        canvases
                            .values()
                            .map(|canvas| canvas.pixels.capacity() + canvas.stencil.capacity())
                            .sum::<usize>(),
                    )
                };
                let (batch_count, batch_entries) = {
                    let batches = state.sprite_batches.lock();
                    (
                        batches.len(),
                        batches
                            .values()
                            .map(|batch| batch.entries.capacity())
                            .sum::<usize>(),
                    )
                };
                eprintln!(
                    "[resources] images={} {:.1}MiB canvases={} {:.1}MiB batches={} entries={}",
                    image_count,
                    image_bytes as f64 / 1_048_576.0,
                    canvas_count,
                    canvas_bytes as f64 / 1_048_576.0,
                    batch_count,
                    batch_entries
                );
                let phases: String = lua
                    .load(include_str!("diagnostics/checkpoints.lua"))
                    .eval()
                    .unwrap_or_else(|error| format!("profile error: {error}"));
                eprintln!("[phases] {phases}");
                let draw_owners: String = lua
                    .load(include_str!("diagnostics/draw_owners.lua"))
                    .eval()
                    .unwrap_or_else(|error| format!("draw owner profile error: {error}"));
                eprintln!("[draw-owners] {draw_owners}");
                let move_owners: String = lua
                    .load(include_str!("diagnostics/move_owners.lua"))
                    .eval()
                    .unwrap_or_else(|error| format!("move owner profile error: {error}"));
                eprintln!("[move-owners] {move_owners}");
                let update_owners: String = lua
                    .load(include_str!("diagnostics/update_owners.lua"))
                    .eval()
                    .unwrap_or_else(|error| format!("update owner profile error: {error}"));
                eprintln!("[update-owners] {update_owners}");
                let jit_profile: String = lua
                    .load(include_str!("diagnostics/jit.lua"))
                    .eval()
                    .unwrap_or_else(|error| format!("profile error: {error}"));
                if !jit_profile.is_empty() {
                    eprintln!("[jit] {jit_profile}");
                }
                let lua_objects: String = lua
                    .load(include_str!("diagnostics/memory.lua"))
                    .eval()
                    .unwrap_or_else(|error| format!("object profile error: {error}"));
                eprintln!("[objects] {lua_objects}");
                lua.load("if G and G.ARGS then G.ARGS.FUNC_TRACKER = {} end")
                    .exec()
                    .ok();
            }
        }

        if frame_limit.is_some_and(|limit| frame_count >= limit) {
            if let Some(output) = &mut queued_present {
                output.finish()?;
            }
            if render_mode == RenderMode::Framebuffer {
                crate::platform::finish_presentation()?;
            }
            if let Some(path) = std::env::var_os("BALATRO_TEST_SNAPSHOT") {
                let path = path.to_string_lossy();
                state.pixel_buffer.lock().save_ppm(&path)?;
                eprintln!("[runtime] saved test snapshot: {path}");
            }
            let hold_ms = std::env::var("BALATRO_TEST_HOLD_MS")
                .ok()
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or(0);
            if hold_ms > 0 {
                eprintln!("[runtime] test frame ready; holding for {hold_ms}ms");
                std::thread::sleep(Duration::from_millis(hold_ms));
            }
            eprintln!("[runtime] test frame limit reached: {frame_count}");
            break;
        }

        // Frame limiter
        let elapsed = frame_start.elapsed();
        if elapsed < frame_duration {
            std::thread::sleep(frame_duration - elapsed);
        }
    }
    if let Some(output) = &mut queued_present {
        output.finish()?;
        output.report();
    }
    benchmark.report();
    if render_mode == RenderMode::Framebuffer {
        crate::platform::finish_presentation()?;
    }
    #[cfg(feature = "shader-cache-stats")]
    {
        let buffer = state.pixel_buffer.lock();
        eprintln!(
            "[shader-cache-stats] colour={:?} spatial={:?}",
            buffer.shader_colour_cache_stats(),
            buffer.shader_spatial_cache_stats()
        );
    }
    Ok(())
}

/// Fallback loop for standard LÖVE games (love.load → love.update → love.draw)
fn run_fallback_loop<B: Backend>(
    terminal: &mut Terminal<B>,
    lua: &Lua,
    state: &Arc<SharedState>,
    render_mode: RenderMode,
) -> Result<()> {
    let frame_duration = Duration::from_nanos((1_000_000_000.0 / TARGET_FPS) as u64);
    let love_table: LuaTable = lua.globals().get("love")?;

    // Call love.load()
    if let Ok(load_fn) = love_table.get::<LuaFunction>("load") {
        if let Err(e) = load_fn.call::<()>(()) {
            eprintln!("[WARN] love.load() error: {}", e);
        }
    }

    loop {
        let frame_start = Instant::now();

        if render_mode == RenderMode::Framebuffer {
            crate::platform::poll_input(state)?;
        }

        // Process events
        if let Ok(pump) = love_table
            .get::<LuaTable>("event")
            .and_then(|ev| ev.get::<LuaFunction>("pump"))
        {
            pump.call::<()>(()).ok();
        }

        // Drain events through handlers
        if let Ok(poll) = love_table
            .get::<LuaTable>("event")
            .and_then(|ev| ev.get::<LuaFunction>("poll"))
        {
            if let Ok(iter_fn) = poll.call::<LuaFunction>(()) {
                loop {
                    let result: LuaMultiValue = iter_fn.call(())?;
                    if result.is_empty() {
                        break;
                    }
                    // Check for quit
                    if let Some(LuaValue::String(name)) = result.get(0) {
                        if name.to_string_lossy() == "quit" {
                            return Ok(());
                        }
                        // Dispatch to handler
                        if let Ok(handlers) = love_table.get::<LuaTable>("handlers") {
                            if let Ok(handler) =
                                handlers.get::<LuaFunction>(name.to_string_lossy().to_string())
                            {
                                let args: Vec<LuaValue> = result.into_iter().skip(1).collect();
                                handler.call::<()>(LuaMultiValue::from_vec(args)).ok();
                            }
                        }
                    }
                }
            }
        }

        if *state.should_quit.lock() {
            break;
        }

        // love.update(dt)
        let dt = {
            let now = Instant::now();
            let mut last = state.last_step_time.lock();
            let dt = last.elapsed().as_secs_f64();
            *last = now;
            *state.last_dt.lock() = dt as f32;
            dt
        };

        if let Ok(update_fn) = love_table.get::<LuaFunction>("update") {
            if let Err(e) = update_fn.call::<()>(dt) {
                eprintln!("[ERROR] love.update: {}", e);
                break;
            }
        }

        // love.draw()
        {
            state.flush_render_jobs();
            let bg = *state.background_color.lock();
            state.pixel_buffer.lock().clear(bg[0], bg[1], bg[2], bg[3]);
        }

        if let Ok(draw_fn) = love_table.get::<LuaFunction>("draw") {
            if let Err(e) = draw_fn.call::<()>(()) {
                eprintln!("[ERROR] love.draw: {}", e);
                break;
            }
        }

        // Render to terminal
        render_frame(terminal, state, render_mode)?;

        // FPS
        state
            .fps_counter
            .lock()
            .update(frame_start.elapsed().as_secs_f32());

        let elapsed = frame_start.elapsed();
        if elapsed < frame_duration {
            std::thread::sleep(frame_duration - elapsed);
        }
    }
    Ok(())
}

// --- Async Sixel writer ---
// Single-slot buffer: render thread encodes a frame and drops it into the slot.
// Writer thread picks it up and writes to stdout. If render produces a new frame
// before writer finishes, the old pending frame is replaced (latest-wins).
// This decouples encode (~12ms) from write (~47ms).
static SIXEL_WRITER: std::sync::LazyLock<(std::sync::Mutex<Option<Vec<u8>>>, std::sync::Condvar)> =
    std::sync::LazyLock::new(|| (std::sync::Mutex::new(None), std::sync::Condvar::new()));

fn start_sixel_writer_thread() {
    use std::io::Write;
    std::thread::Builder::new()
        .name("sixel-writer".into())
        .spawn(move || {
            let (lock, cvar) = &*SIXEL_WRITER;
            loop {
                let frame = {
                    let mut slot = lock.lock().unwrap();
                    while slot.is_none() {
                        slot = cvar.wait(slot).unwrap();
                    }
                    slot.take().unwrap()
                };
                let mut stdout = std::io::stdout().lock();
                let _ = stdout.write_all(&frame);
                let _ = stdout.flush();
            }
        })
        .expect("Failed to spawn sixel-writer thread");
}

fn render_frame<B: Backend>(
    terminal: &mut Terminal<B>,
    state: &Arc<SharedState>,
    render_mode: RenderMode,
) -> Result<()> {
    state.flush_render_jobs();
    let pb = state.pixel_buffer.lock();
    match render_mode {
        RenderMode::Octant => {
            terminal.draw(|frame| {
                frame.render_widget(OctantWidget { buf: &pb }, frame.area());
            })?;
        }
        RenderMode::HalfBlock => {
            terminal.draw(|frame| {
                frame.render_widget(PixelWidget { buf: &pb }, frame.area());
            })?;
        }
        RenderMode::Sixel => {
            use std::sync::atomic::{AtomicU32, Ordering};

            // Skip encoding if writer hasn't consumed previous frame
            {
                let slot = SIXEL_WRITER.0.lock().unwrap();
                if slot.is_some() {
                    return Ok(());
                }
            }

            // Track last rendered size to clear screen on resize
            static LAST_SIZE: AtomicU32 = AtomicU32::new(0);
            let cur_size = ((pb.width as u32) << 16) | (pb.height as u32);
            let prev = LAST_SIZE.swap(cur_size, Ordering::Relaxed);

            let target_w = *state.sixel_text_w.lock();
            let target_h = *state.sixel_text_h.lock();

            let t0 = std::time::Instant::now();
            let mut sixel_data = encode_sixel(&pb, target_w, target_h);
            let t_encode = t0.elapsed();
            let sixel_size = sixel_data.len();

            // Build frame: header + sixel_data (append is O(1)) + footer
            let mut frame_buf = Vec::with_capacity(sixel_size + 32);
            frame_buf.extend_from_slice(b"\x1b[?2026h");
            if prev != 0 && prev != cur_size {
                frame_buf.extend_from_slice(b"\x1b[2J");
            }
            frame_buf.extend_from_slice(b"\x1b[H");
            frame_buf.append(&mut sixel_data);
            frame_buf.extend_from_slice(b"\x1b[?2026l");

            // Send to async writer thread (non-blocking, instant wakeup via condvar).
            {
                let mut slot = SIXEL_WRITER.0.lock().unwrap();
                *slot = Some(frame_buf);
                SIXEL_WRITER.1.notify_one();
            }

            let t_total = t0.elapsed();
            static SIXEL_FRAME: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
            let sf = SIXEL_FRAME.fetch_add(1, Ordering::Relaxed);
            if sf % 60 == 0 {
                eprintln!(
                    "[SIXEL] encode={:.1}ms queue={:.1}ms size={}KB canvas={}x{} sixel={}x{}",
                    t_encode.as_secs_f64() * 1000.0,
                    t_total.as_secs_f64() * 1000.0,
                    sixel_size / 1024,
                    pb.width,
                    pb.height,
                    target_w,
                    target_h
                );
            }
        }
        RenderMode::Framebuffer => crate::platform::present(&pb)?,
        RenderMode::Headless => {}
    }
    Ok(())
}
