// Derived from balatro-port-tui (Apache-2.0). Modified by Producdevity; see NOTICE.

mod benchmark;
#[cfg(all(feature = "copy-trace", target_os = "linux", target_arch = "arm"))]
mod copy_trace;
#[cfg(test)]
mod diagnostics_tests;
mod incremental_gc;
mod lua_jit;
mod lua_sampler;
mod miyoo_input;
mod platform;
mod queued_present;
mod runner;
mod setup;

use std::path::PathBuf;

fn main() -> anyhow::Result<()> {
    // Declare DPI awareness BEFORE anything else — Windows virtualizes DPI to 96
    // for non-aware processes, making cell size detection wrong.
    #[cfg(windows)]
    unsafe {
        extern "system" {
            fn SetProcessDpiAwarenessContext(value: isize) -> i32;
            fn SetProcessDPIAware() -> i32;
        }
        // Try modern API first (Win10 1703+), fall back to legacy (Vista+)
        if SetProcessDpiAwarenessContext(-4) == 0 {
            // DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2
            SetProcessDPIAware();
        }
    }

    let mut args = std::env::args_os().skip(1);
    let first = args.next().ok_or_else(|| {
        anyhow::anyhow!(
            "Usage: balatro-runtime GAME | --onion DIRECTORY | --prepare-audio GAME OUTPUT"
        )
    })?;
    if first == "--version" {
        println!("Balatro for Miyoo Mini {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    if first == "--prepare-audio" {
        let game = args
            .next()
            .map(PathBuf::from)
            .ok_or_else(|| anyhow::anyhow!("Missing game path"))?;
        let output = args
            .next()
            .map(PathBuf::from)
            .ok_or_else(|| anyhow::anyhow!("Missing audio cache path"))?;
        anyhow::ensure!(args.next().is_none(), "Too many arguments");
        return love_api::audio::prepare_cache(&game, &output);
    }
    let game_path = if first == "--onion" {
        let directory = args
            .next()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        anyhow::ensure!(args.next().is_none(), "Too many arguments");
        setup::prepare(&directory)?
    } else {
        anyhow::ensure!(args.next().is_none(), "Too many arguments");
        PathBuf::from(first)
    };

    if !game_path.exists() {
        eprintln!("Error: game path does not exist: {}", game_path.display());
        std::process::exit(1);
    }

    eprintln!(
        "[runtime] port={} game={}",
        env!("CARGO_PKG_VERSION"),
        game_path.display()
    );
    runner::run(&game_path)
}
