//! Artwork baked into the guest NRO and used directly by the host oracle.
use crate::sprites::*;
use std::{vec, vec::Vec};
use core::f32::consts::{FRAC_1_SQRT_2, TAU};

pub const SCROLL_PER_SECOND: f32 = 120.0;
const PARALLAX: [f32; 3] = [0.15, 0.4, 0.8];
const SPRITES_PER_LAYER: usize = 65;
const CELL: usize = ATLAS_SIZE / 4;
const HILLS: [[u8; 3]; 3] = [[150, 182, 205], [92, 140, 112], [40, 82, 62]];
pub const SHAPES: [[u8; 3]; 4] = [[255, 214, 92], [236, 92, 92], [255, 255, 255], [112, 200, 255]];
const SUN: [f32; 4] = [1080.0, 90.0, 48.0, 48.0];

// Rows CELL * k.. hold hill k, opaque below a two-harmonic ridge with k + 1 periods across the row so it wraps seamlessly,
// shaded darker towards the bottom; the last CELL rows hold a disc, a ring, a star and a diamond. Edges are a few texels
// wide so the linear filter stays smooth when the 2048 texels are minified.
pub fn atlas() -> Vec<u8> {
    let scale = CELL as f32 / 64.0;
    let texel = |x: usize, y: usize| {
        let [cx, cy] = [x % CELL, y % CELL].map(|c| c as f32 + 0.5 - CELL as f32 / 2.0);
        let (rgb, distance, edge, shade) = match y / CELL {
            row @ 0..3 => {
                let k = row as f32;
                let phase = TAU * (k + 1.0) * (x as f32 + 0.5) / ATLAS_SIZE as f32;
                let ridge = (10.0 + 6.0 * k) * scale * (phase.sin() + 0.3 * (3.0 * phase + k).sin());
                (HILLS[row], ridge - cy, 3.0, 1.0 - (0.3 * (cy - ridge) / (CELL as f32 / 2.0)).clamp(0.0, 0.3))
            }
            _ => {
                let r = cx.hypot(cy);
                let distance = match x / CELL {
                    0 => r - 28.0 * scale,
                    1 => (r - 22.0 * scale).abs() - 6.0 * scale,
                    2 => r - (18.0 + 9.0 * (5.0 * cy.atan2(cx)).cos()) * scale,
                    _ => (cx.abs() + cy.abs() - 28.0 * scale) * FRAC_1_SQRT_2,
                };
                (SHAPES[x / CELL], distance, 10.0, 1.0)
            }
        };
        let [r, g, b] = rgb.map(|c| (f32::from(c) * shade).round() as u8);
        [r, g, b, ((0.5 - distance / edge).clamp(0.0, 1.0) * 255.0).round() as u8]
    };
    (0..ATLAS_SIZE * ATLAS_SIZE).flat_map(|i| texel(i % ATLAS_SIZE, i / ATLAS_SIZE)).collect()
}

/// Per instance: transform (centre, half size), uv rect (origin, size) and layer (position parallax, uv parallax,
/// rotation, alpha); the sun first, then each hill followed by the sprites in front of it.
pub fn instances(seconds: f32) -> Vec<[f32; 12]> {
    let scroll = seconds * SCROLL_PER_SECOND;
    let cell = |shape: usize| [0.25 * shape as f32, 0.75, 0.25, 0.25];
    let mut instances = vec![[SUN, cell(0), [0.0, 0.0, 0.0, 0.8]].concat().try_into().unwrap()];
    for (k, parallax) in PARALLAX.into_iter().enumerate() {
        let depth = k as f32;
        let hill = [640.0, [520.0, 560.0, 620.0][k], 640.0, [200.0, 160.0, 100.0][k], 0.0, 0.25 * depth, 3.0 - 0.5 * depth, 0.25, 0.0, parallax, 0.0, 1.0];
        instances.push(hill);
        for i in 0..SPRITES_PER_LAYER {
            let n = (k * SPRITES_PER_LAYER + i) as f32;
            let half = (6.0 + 6.0 * depth) * (1.0 + 0.6 * (n * 0.31).fract());
            // The host wraps each sprite back across the screen; the vertex program subtracts the parallax scroll.
            let period = SCENE[0] + 128.0;
            let x = ((n * 0.618_034).fract() * period - scroll * parallax).rem_euclid(period) - 64.0 + scroll * parallax;
            let y = 160.0 + (n * 0.414_214).fract() * 300.0 + 10.0 * (1.3 * seconds + n).sin();
            let spin = if i % 2 == 0 { 1.0 } else { -1.0 } * (0.5 + (n * 0.43).fract());
            let layer = [parallax, 0.0, 0.37 * n + spin * seconds, 0.6 + 0.175 * depth];
            instances.push([[x, y, half, half], cell(i % 4), layer].concat().try_into().unwrap());
        }
    }
    instances
}

