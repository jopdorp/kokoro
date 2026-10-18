#![no_std]
#![no_main]

include!(concat!(env!("OUT_DIR"), "/boombox.rs"));
const DATA: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/boombox.bin"));
include!("../pbr.rs");
