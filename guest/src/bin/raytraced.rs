#![no_std]
#![no_main]

include!(concat!(env!("OUT_DIR"), "/raytraced.rs"));
const _: () = assert!(ROOM && TRACED);
const DATA: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/raytraced.bin"));
const AUDIO: &[u8] = include_bytes!("../../../demos/assets/boombox-lofi/lofi.pcm");
include!("../pbr.rs");
