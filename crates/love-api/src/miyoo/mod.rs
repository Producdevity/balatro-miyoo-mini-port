pub(crate) mod patches;

pub(super) const SCALAR_COLLISION: &str = include_str!("scalar_collision.lua");

#[cfg(test)]
mod tests;

#[cfg(test)]
mod menu_clock_tests;
