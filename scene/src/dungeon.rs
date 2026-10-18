//! Deterministic dungeon world data shared by the host oracle and guest build.

pub const WORLD_SEED: u32 = 8;
pub const VERTEX_BYTES: usize = 48;
pub const PROP_WORDS: usize = 8;

pub mod sound;

#[cfg(feature = "std")]
pub mod data;
pub mod compute;
