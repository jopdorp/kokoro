#![no_std]
#![no_main]

include!(concat!(env!("OUT_DIR"), "/raytraced.rs"));
const DATA: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/raytraced.bin"));
const BOOMBOX: bool = false;
const AUDIO: &[u8] = &[];
include!("../pbr.rs");
