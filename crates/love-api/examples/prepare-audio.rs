use anyhow::{Context, Result};
use std::path::PathBuf;

fn main() -> Result<()> {
    let mut args = std::env::args_os().skip(1);
    let game = PathBuf::from(args.next().context("usage: prepare-audio GAME OUTPUT")?);
    let output = PathBuf::from(args.next().context("usage: prepare-audio GAME OUTPUT")?);
    anyhow::ensure!(args.next().is_none(), "usage: prepare-audio GAME OUTPUT");
    love_api::audio::prepare_cache(&game, &output)
}
