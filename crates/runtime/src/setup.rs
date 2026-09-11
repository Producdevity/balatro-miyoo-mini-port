use anyhow::{bail, ensure, Context, Result};
use love_api::audio::{prepare_cache_with_progress, CacheProgress};
use love_api::state::GameSource;
use sprite_to_text::pixel_buffer::PixelBuffer;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const GAME_NAMES: &[&str] = &["Balatro.exe", "Balatro.love", "Balatro"];

fn find_game(directory: &Path) -> Result<PathBuf> {
    let files: Vec<_> = std::fs::read_dir(directory)?.collect::<std::io::Result<Vec<_>>>()?;
    for name in GAME_NAMES {
        let mut matches: Vec<_> = files
            .iter()
            .filter(|entry| {
                entry
                    .file_name()
                    .to_str()
                    .is_some_and(|file| file.eq_ignore_ascii_case(name))
                    && entry.path().is_file()
            })
            .map(|entry| entry.path())
            .collect();
        matches.sort();
        ensure!(
            matches.len() <= 1,
            "More than one {name} was found. Keep only the game file you want to use."
        );
        if let Some(path) = matches.pop() {
            return Ok(path);
        }
    }
    bail!("Copy Balatro.exe or Balatro.love to Roms/PORTS/Games/Balatro, then launch again.")
}

fn validate_game(game: &Path) -> Result<String> {
    let source = GameSource::from_path(game).context("Could not open the game archive")?;
    for name in [
        "main.lua",
        "game.lua",
        "globals.lua",
        "engine/controller.lua",
        "functions/UI_definitions.lua",
    ] {
        ensure!(
            source.file_exists(name),
            "The game archive is missing {name}. Copy the original game file again."
        );
    }
    let version = source
        .read_file("version.jkr")
        .context("The game version file is missing")?;
    let version = std::str::from_utf8(&version)?
        .lines()
        .next()
        .unwrap_or_default();
    ensure!(
        !version.is_empty() && version.len() < 80,
        "The game version file is invalid"
    );
    Ok(version.to_owned())
}

struct Screen {
    visible: bool,
    frame: PixelBuffer,
    last_draw: Option<Instant>,
}

impl Screen {
    fn new() -> Self {
        Self {
            visible: std::env::var("TUI_RENDER").as_deref() == Ok("framebuffer"),
            frame: PixelBuffer::new(640, 480),
            last_draw: None,
        }
    }

    fn draw(&mut self, heading: &str, lines: &[String], progress: Option<CacheProgress>) {
        if !self.visible {
            return;
        }
        if progress.is_some_and(|p| p.completed != p.total)
            && self
                .last_draw
                .is_some_and(|t| t.elapsed() < Duration::from_millis(200))
        {
            return;
        }
        self.frame.clear(0.09, 0.16, 0.14, 1.0);
        self.frame
            .draw_text_scaled("Balatro", 32, 32, 4.0, [238, 226, 197, 255]);
        self.frame
            .draw_text_scaled(heading, 32, 112, 2.0, [255, 255, 255, 255]);
        for (index, line) in lines.iter().take(7).enumerate() {
            self.frame.draw_text_scaled(
                line,
                32,
                164 + index as i32 * 28,
                2.0,
                [220, 230, 223, 255],
            );
        }
        if let Some(state) = progress {
            self.frame.fill_rect(32, 390, 576, 18, [64, 87, 78, 255]);
            let width = 576 * state.completed / state.total.max(1);
            self.frame
                .fill_rect(32, 390, width as i32, 18, [221, 176, 72, 255]);
        }
        self.frame.draw_text(
            &format!("v{}", env!("CARGO_PKG_VERSION")),
            32,
            446,
            [166, 190, 175, 255],
        );
        if let Err(error) = crate::platform::present(&self.frame) {
            eprintln!("[setup] display unavailable: {error:#}");
            self.visible = false;
        }
        self.last_draw = Some(Instant::now());
    }

    fn error(&mut self, error: &anyhow::Error) {
        let message = format!("{error}");
        let mut lines = wrap(&message, 36);
        if lines.len() > 4 {
            lines.truncate(4);
        }
        lines.push(String::new());
        lines.push("Details are in launcher.log".to_owned());
        self.draw("Setup could not finish", &lines, None);
        if self.visible {
            let _ = crate::platform::finish_presentation();
            std::thread::sleep(Duration::from_secs(8));
        }
    }
}

fn wrap(text: &str, columns: usize) -> Vec<String> {
    assert!(columns > 0);
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        let chars: Vec<_> = word.chars().collect();
        for part in chars.chunks(columns) {
            if !line.is_empty() && line.chars().count() + 1 + part.len() > columns {
                lines.push(std::mem::take(&mut line));
            }
            if !line.is_empty() {
                line.push(' ');
            }
            line.extend(part);
        }
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

pub fn prepare(directory: &Path) -> Result<PathBuf> {
    let mut screen = Screen::new();
    screen.draw("Checking game files", &[], None);
    let result = (|| {
        let game = find_game(directory)?;
        let version = validate_game(&game)?;
        eprintln!(
            "[setup] port={} game={version} file={}",
            env!("CARGO_PKG_VERSION"),
            game.display()
        );
        let output = std::env::var_os("BALATRO_PCM_CACHE")
            .map(PathBuf::from)
            .unwrap_or_else(|| directory.join("audio-cache"));
        let mut logged = Instant::now();
        let start = Instant::now();
        let summary = prepare_cache_with_progress(&game, &output, |state| {
            screen.draw(
                "Preparing audio",
                &[
                    "This only needs to run once.".to_owned(),
                    format!(
                        "Sound {} of {}",
                        state.completed.min(state.total),
                        state.total
                    ),
                    format!("{} MiB prepared", state.bytes / (1024 * 1024)),
                ],
                Some(state),
            );
            if logged.elapsed() >= Duration::from_secs(2) {
                eprintln!(
                    "[setup] audio={}/{} bytes={}",
                    state.completed, state.total, state.bytes
                );
                logged = Instant::now();
            }
        })
        .context("Audio preparation failed. Check free space on the SD card, then launch again.")?;
        std::env::set_var("BALATRO_PCM_CACHE", output);
        eprintln!(
            "[setup] ready reused={} files={} bytes={} elapsed={:.2}s",
            summary.reused,
            summary.files,
            summary.bytes,
            start.elapsed().as_secs_f64()
        );
        screen.draw("Starting game", &[], None);
        Ok(game)
    })();
    if let Err(error) = &result {
        eprintln!("[setup] {error:#}");
        screen.error(error);
    }
    result
}

#[cfg(test)]
#[path = "setup_tests.rs"]
mod tests;
