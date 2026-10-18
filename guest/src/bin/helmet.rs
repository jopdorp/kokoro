#![no_std]
#![no_main]

include!(concat!(env!("OUT_DIR"), "/helmet.rs"));
const DATA: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/helmet.bin"));
const AUDIO: &[u8] = &[];
include!("../pbr.rs");
