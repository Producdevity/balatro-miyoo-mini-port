mod framebuffer;
mod input;

use anyhow::Result;
use love_api::state::SharedState;
use sprite_to_text::pixel_buffer::PixelBuffer;

pub fn present(frame: &PixelBuffer) -> Result<()> {
    framebuffer::present(frame)
}

pub fn poll_input(state: &SharedState) -> Result<()> {
    input::poll(state)
}

pub fn finish_presentation() -> Result<()> {
    framebuffer::finish()
}
