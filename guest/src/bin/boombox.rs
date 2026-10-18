#![no_std]
#![no_main]

include!(concat!(env!("OUT_DIR"), "/boombox.rs"));
const DATA: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/boombox.bin"));
const BOOMBOX: bool = true;
const AUDIO: &[u8] = include_bytes!("../../../demos/assets/boombox-lofi/lofi.pcm");
include!("../pbr.rs");
