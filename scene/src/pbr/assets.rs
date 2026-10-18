//! Converted model assets laid out for the PBR encoder.
use std::{prelude::v1::*, vec, format};
use super::*;
use crate::sass::bra;
const PAGE: u64 = 0x1_0000;
fn bytes(words: &[u32]) -> Vec<u8> { words.iter().flat_map(|v| v.to_le_bytes()).collect() }
/// A minimal reader for the converter's manifest.json: numbers, strings without escapes, arrays and objects.
enum Json {
    Number(f64),
    Text(String),
    List(Vec<Json>),
    Map(Vec<(String, Json)>),
    Other,
}

impl Json {
    fn parse(text: &mut &str) -> Option<Json> {
        *text = text.trim_start();
        let open = text.chars().next()?;
        if let Some(rest) = text.strip_prefix('"') {
            let (value, rest) = rest.split_once('"')?;
            *text = rest;
            return Some(Json::Text(value.into()));
        }
        if !matches!(open, '{' | '[') {
            let end = text.find([',', '}', ']']).unwrap_or(text.len());
            let value = text[..end].trim().parse().map_or(Json::Other, Json::Number);
            *text = &text[end..];
            return Some(value);
        }
        *text = &text[1..];
        let mut items = Vec::new();
        loop {
            *text = text.trim_start().trim_start_matches(',').trim_start();
            if let Some(rest) = text.strip_prefix(['}', ']']) {
                *text = rest;
                break;
            }
            let key = match open {
                '{' => {
                    let Json::Text(key) = Json::parse(text)? else { return None };
                    *text = text.trim_start().strip_prefix(':')?;
                    key
                }
                _ => String::new(),
            };
            items.push((key, Json::parse(text)?));
        }
        Some(if open == '{' { Json::Map(items) } else { Json::List(items.into_iter().map(|(_, value)| value).collect()) })
    }

    fn get(&self, key: &str) -> Result<&Json, String> {
        match self {
            Json::Map(items) => items.iter().find(|(k, _)| k == key).map(|(_, value)| value).ok_or_else(|| format!("manifest has no {key}")),
            _ => Err(format!("manifest {key} is not inside an object")),
        }
    }

    fn list(&self) -> &[Json] {
        if let Json::List(items) = self { items } else { &[] }
    }

    fn number(&self) -> f32 {
        if let Json::Number(n) = self { *n as f32 } else { 0.0 }
    }

    fn text(&self) -> &str {
        if let Json::Text(s) = self { s } else { "" }
    }
}

// A bindless cbuf handle: the address >> 4 in bits 0..45 and the size >> 4 above (NVK nvk_bindless_cbuf).
fn uniform_handle(va: u64, size: u32) -> [u32; 2] {
    [(va >> 4) as u32, (va >> 36) as u32 | (size >> 4) << 13]
}

fn floats(values: &[f32]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_le_bytes()).collect()
}

fn vertex(out: &mut Vec<u8>, position: [f32; 3], normal: [f32; 3]) {
    let tangent = if normal[0].abs() > 0.9 { [0.0, 0.0, 1.0] } else { [1.0, 0.0, 0.0] };
    out.extend(floats(&[position[0], position[1], position[2], normal[0], normal[1], normal[2], tangent[0], tangent[1], tangent[2], 1.0, 0.0, 0.0]));
}

fn quad(vertices: &mut Vec<u8>, indices: &mut Vec<u32>, corners: [[f32; 3]; 4], normal: [f32; 3]) -> [u32; 2] {
    let base = (vertices.len() / 48) as u32;
    for corner in corners { vertex(vertices, corner, normal); }
    let first = indices.len() as u32;
    indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    [first, 6]
}

fn sphere(vertices: &mut Vec<u8>, indices: &mut Vec<u32>, centre: [f32; 3], radius: f32) -> [u32; 2] {
    const RINGS: u32 = 16;
    const SEGMENTS: u32 = 32;
    let base = (vertices.len() / 48) as u32;
    for ring in 0..=RINGS {
        let latitude = std::f32::consts::PI * ring as f32 / RINGS as f32;
        for segment in 0..=SEGMENTS {
            let longitude = std::f32::consts::TAU * segment as f32 / SEGMENTS as f32;
            let normal = [latitude.sin() * longitude.cos(), latitude.cos(), latitude.sin() * longitude.sin()];
            vertex(vertices, std::array::from_fn(|i| centre[i] + normal[i] * radius), normal);
        }
    }
    let first = indices.len() as u32;
    for ring in 0..RINGS { for segment in 0..SEGMENTS {
        let a = base + ring * (SEGMENTS + 1) + segment;
        let b = a + SEGMENTS + 1;
        indices.extend([a, b, a + 1, a + 1, b, b + 1]);
    }}
    [first, indices.len() as u32 - first]
}

fn align(va: u64) -> u64 {
    va.next_multiple_of(ALIGN)
}

pub struct Asset {
    pub writes: Vec<(u64, Vec<u8>)>,
    pub target: u64,
    pub end: u64,
    pub draws: Vec<[u32; 3]>,
    pub blends: Vec<Option<Vec<u32>>>,
    pub textures: Vec<u64>,
    pub fit: ([f32; 3], f32),
    pub model_vertices: usize,
    pub model_indices: u32,
    pub vertex_count: u32,
    pub index_count: u32,
    pub indices_va: u64,
    pub light_count: u32,
    pub light_first: u32,
    pub material_count: usize,
    pub texture_count: u32,
}
impl Asset {
pub fn load(read: &dyn Fn(&str) -> Result<Vec<u8>, String>, width: u32, height: u32, room: bool, traced: bool, showcase: bool) -> Result<Self, String> {
        let text = String::from_utf8(read("manifest.json")?).map_err(|e| e.to_string())?;
        let manifest = Json::parse(&mut text.as_str()).ok_or("manifest.json does not parse")?;
        let [mut vertices, mut indices] = [read("vertices.bin")?, read("indices.bin")?];
        let (model_vertices, model_indices) = (vertices.len() / 48, indices.len() / 4);
        let mut room_draws = Vec::new();
        if room {
            let mut room_indices = Vec::new();
            for (corners, normal) in [
                ([[-3.0, -1.0, -3.0], [3.0, -1.0, -3.0], [3.0, -1.0, 3.0], [-3.0, -1.0, 3.0]], [0.0, 1.0, 0.0]),
                ([[-3.0, -1.0, 3.0], [-3.0, -1.0, -3.0], [-3.0, 3.0, -3.0], [-3.0, 3.0, 3.0]], [1.0, 0.0, 0.0]),
                ([[3.0, -1.0, -3.0], [3.0, -1.0, 3.0], [3.0, 3.0, 3.0], [3.0, 3.0, -3.0]], [-1.0, 0.0, 0.0]),
                ([[-3.0, -1.0, -3.0], [3.0, -1.0, -3.0], [3.0, 3.0, -3.0], [-3.0, 3.0, -3.0]], [0.0, 0.0, 1.0]),
                ([[-3.0, 3.0, -3.0], [3.0, 3.0, -3.0], [3.0, 3.0, 3.0], [-3.0, 3.0, 3.0]], [0.0, -1.0, 0.0]),
            ] { room_draws.push(quad(&mut vertices, &mut room_indices, corners, normal)); }
            room_draws.push(sphere(&mut vertices, &mut room_indices, [-1.65, -0.35, -0.55], 0.65));
            room_draws.push(sphere(&mut vertices, &mut room_indices, [1.65, -0.35, -0.45], 0.65));
            room_draws.push(sphere(&mut vertices, &mut room_indices, [0.0; 3], 0.12));
            indices.extend(bytes(&room_indices));
        }
        let textures = manifest.get("textures")?.list();
        let indices_va = align(VERTICES + vertices.len() as u64);
        let mut texture_va = vec![align(indices_va + indices.len() as u64)];
        for texture in textures {
            texture_va.push(align(texture_va.last().unwrap() + texture.get("bytes")?.number() as u64));
        }
        let room_textures = texture_va.pop().unwrap();
        let target = room_textures + if room { ALIGN } else { 0 };
        let end = (target + u64::from(width * height * 4)).next_multiple_of(PAGE);
        let mut writes = Vec::new();
        for (va, data) in [(VERTICES, &vertices), (indices_va, &indices)] {
            writes.push((va, data.clone()));
        }

        const A8B8G8R8_UNORM: u32 = 8 | 0o2222 << 7 | 0o5432 << 19;
        const BLOCK_LINEAR: u32 = 3 << 21;
        const TWO_D: u32 = 1 << 23;
        const NORMALIZED: u32 = 1 << 31;
        let mut tic = Vec::new();
        for (texture, &va) in textures.iter().zip(&texture_va) {
            writes.push((va, read(texture.get("file")?.text())?));
            let field = |key| texture.get(key).map(|value| value.number() as u32);
            let [width, height, mips, gob_height_log2] = [field("width")?, field("height")?, field("mips")?, field("gob_height_log2")?];
            let [hi, lo] = split(va);
            tic.extend([A8B8G8R8_UNORM, lo, hi | BLOCK_LINEAR, gob_height_log2 << 3 | (mips - 1) << 28, (width - 1) | TWO_D, (height - 1) | NORMALIZED, 0, 0]);
        }
        if room {
            for (i, pixel) in ROOM_MATERIALS.iter().flat_map(|(base, metallic, roughness)| [
                [base[0], base[1], base[2], 255], [128, 128, 255, 255], [0, (roughness * 255.0) as u8, (metallic * 255.0) as u8, 255], [255; 4], [base[0], base[1], base[2], 255],
            ]).enumerate() {
                let va = room_textures + i as u64 * 512;
                writes.push((va, (pixel).to_vec()));
                let [hi, lo] = split(va);
                tic.extend([A8B8G8R8_UNORM, lo, hi | BLOCK_LINEAR, 0, TWO_D, NORMALIZED, 0, 0]);
            }
        }
        writes.push((TIC, bytes(&tic)));
        // Repeat, trilinear, 16x anisotropic, LOD clamped to 0..15.
        const MAX_ANISOTROPY_16: u32 = 7 << 20;
        const TRILINEAR: u32 = 2 | 2 << 4 | 3 << 6;
        writes.push((TSC, (bytes(&[MAX_ANISOTROPY_16, TRILINEAR, 0xf00 << 12, 0, 0, 0, 0, 0])).to_vec()));

        let light = [0.4f32, 0.8, 0.45];
        let light = light.map(|c| c / light.iter().map(|c| c * c).sum::<f32>().sqrt());
        let texture_index = |file: &str| textures.iter().position(|t| t.get("file").is_ok_and(|f| f.text() == file)).map(|i| i as u32);
        let materials = manifest.get("materials")?.list();
        let mut blends: Vec<Option<Vec<u32>>> = materials.iter().map(|m| m.get("blend").ok().map(|b| b.list().iter().map(|v| v.number() as u32).collect())).collect();
        let uniform_pointer = if traced { 0x28 / 4 } else { UNIFORM_POINTER };
        let eye = if room { ROOM_EYE } else { EYE };
        if room { blends.extend(std::iter::repeat_n(None, ROOM_MATERIALS.len())); }
        for (m, material) in materials.iter().enumerate() {
            let maps = material.get("textures")?;
            let base = maps.get("baseColor")?.text();
            let mut table = [0; 12];
            for (k, slot) in SLOTS.into_iter().enumerate() {
                let file = maps.get(slot).map_or(base, Json::text);
                table[k] = texture_index(file).ok_or_else(|| format!("material {m} {slot} {file} is not in textures"))?;
            }
            let va = MATERIALS + MATERIAL_STRIDE * m as u64;
            [table[uniform_pointer], table[uniform_pointer + 1]] = uniform_handle(va + MATERIAL_UNIFORMS, if traced { 128 } else { 112 });
            if traced { [table[6], table[7]] = [TLAS as u32, (TLAS >> 32) as u32]; }
            let numbers = |key| material.get(key).map(|value| value.list().iter().map(Json::number).collect::<Vec<_>>());
            let [base_factor, mut emissive] = [numbers("baseColorFactor")?, numbers("emissiveFactor")?];
            emissive.resize(4, 0.0);
            let scalar = |key| material.get(key).map(Json::number);
            let factors = [scalar("metallicFactor")?, scalar("roughnessFactor")?, scalar("normalScale")?, scalar("occlusionStrength")?];
            let default_light = [&light[..], &[0.0, 3.0, 2.9, 2.7, 0.0, 0.6, 0.62, 0.66, 0.0]].concat();
            let lights = numbers("light").unwrap_or(default_light);
            let mut uniforms = [&lights[..], &eye, &[1.0], &base_factor, &emissive, &factors].concat();
            if traced { uniforms.extend([0.35, 0.45, 0.6, 0.002]); }
            writes.push((va, bytes(&table)));
            writes.push((va + MATERIAL_UNIFORMS, floats(&uniforms)));
        }
        if room {
            for (i, (_, metallic, roughness)) in ROOM_MATERIALS.iter().enumerate() {
                let va = MATERIALS + MATERIAL_STRIDE * (materials.len() + i) as u64;
                let t = textures.len() as u32 + i as u32 * 5;
                let mut table = [t, t + 1, t + 2, t + 3, t + 4, 0, 0, 0, 0, 0, 0, 0];
                if traced { [table[6], table[7]] = [TLAS as u32, (TLAS >> 32) as u32]; }
                [table[uniform_pointer], table[uniform_pointer + 1]] = uniform_handle(va + MATERIAL_UNIFORMS, if traced { 128 } else { 112 });
                let emissive = if i + 1 == ROOM_MATERIALS.len() { [12.0, 8.0, 3.0, 0.0] } else { [0.0; 4] };
                let mut uniforms = [&light[..], &[0.0, 3.0, 2.9, 2.7, 0.0, 0.18, 0.18, 0.2, 0.0], &eye, &[1.0], &[1.0; 4], &emissive, &[*metallic, *roughness, 1.0, 1.0]].concat();
                if traced { uniforms.extend([0.65, 0.75, 0.9, 0.002]); }
                writes.push((va, bytes(&table)));
                writes.push((va + MATERIAL_UNIFORMS, floats(&uniforms)));
            }
            if traced { writes.push((RAY_CONSTANTS, (0x20u32.to_le_bytes()).to_vec())); }
        }
        let mut vertex_table = [0; 12];
        [vertex_table[uniform_pointer], vertex_table[uniform_pointer + 1]] = uniform_handle(VERTEX_UNIFORMS, 128);
        writes.push((VERTEX_TABLE, bytes(&vertex_table)));
        // The rig's programs stop at their last EXIT; the self-branch a compiled program ends with marks the end.
        let programs: [&[u8]; 2] = if traced {
            let pixel: &[u8] = if showcase { include_bytes!("../../../core/tests/fixtures/driver-shaders/rt.frag.bin") } else { include_bytes!("../../../core/tests/fixtures/driver-shaders/rt-realtime.frag.bin") };
            [include_bytes!("../../../core/tests/fixtures/driver-shaders/rt.vert.bin"), pixel]
        } else {
            [include_bytes!("../../../core/tests/fixtures/driver-shaders/pbr.vert.bin"), include_bytes!("../../../core/tests/fixtures/driver-shaders/pbr.frag.bin")]
        };
        for (va, program) in [VERTEX_PROGRAM, PIXEL_PROGRAM].into_iter().zip(programs) {
            writes.push((va, [program, &bra(-16).to_le_bytes()].concat()));
        }

        let mut draws = manifest
            .get("primitives")?
            .list()
            .iter()
            .map(|p| Ok([p.get("first_index")?.number() as u32, p.get("index_count")?.number() as u32, p.get("material")?.number() as u32]))
            .collect::<Result<Vec<_>, String>>()?;
        let light_count = room_draws.last().map_or(0, |draw| draw[1]);
        let light_first = room_draws.last().map_or(0, |draw| model_indices as u32 + draw[0]);
        if room { draws.extend(room_draws.into_iter().enumerate().map(|(material, [first, count])| [model_indices as u32 + first, count, (materials.len() + material) as u32])); }
        let positions: Vec<[f32; 3]> = vertices[..model_vertices * 48].chunks(48).map(|v| std::array::from_fn(|i| f32::from_le_bytes(v[4 * i..4 * i + 4].try_into().unwrap()))).collect();
        let [low, high] = [f32::min, f32::max].map(|pick| std::array::from_fn::<f32, 3, _>(|i| positions.iter().map(|p| p[i]).reduce(pick).unwrap_or(0.0)));
        let extent = (0..3).map(|i| high[i] - low[i]).fold(0.0, f32::max);
        let mut fit = (std::array::from_fn(|i| (low[i] + high[i]) / 2.0), 2.0 / extent);
        if room { fit.0[1] = low[1] + 1.0 / fit.1; }

        let mut words = Vec::new();
        init(width, height, target, vertices.len() as u64, indices_va, (tic.len() / 8) as u32, &mut words);
        writes.push((PUSHBUFFER, bytes(&words)));
        Ok(Self { writes, target, end, draws, blends, textures: texture_va, fit, model_vertices, model_indices: model_indices as u32, vertex_count: (vertices.len()/48) as u32, index_count: (indices.len()/4) as u32, indices_va, light_count, light_first, material_count: materials.len() + if room { ROOM_MATERIALS.len() } else { 0 }, texture_count: (tic.len() / 8) as u32 })
    }
}

pub fn view_projection(width: u32, height: u32, eye: [f32; 3]) -> Matrix {
    super::view_projection(width, height, eye, f32::sqrt)
}
