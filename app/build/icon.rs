//! The app icon, drawn from the design's app tile (Settings' nav header: a 48-DIP `--card` tile, radius 8,
//! with a `--card-line` edge and the three `--link` bars 5 wide, 3 apart, 10/22/15 high, 13 up from the
//! bottom), in dark-theme colors, which read on light and dark shells alike. Every standard size is drawn
//! from the geometry, antialiased, so small ones stay sharp; 48 and up are PNGs (smaller), the rest 32-bit DIBs.

use crate::png;

const SIZES: [u32; 9] = [16, 20, 24, 32, 40, 48, 64, 96, 256];
/// Design dark `--card`, `--card-line`, `--link`.
const CARD: [u8; 3] = [0x2B, 0x2B, 0x2B];
const CARD_LINE: [u8; 3] = [0x1D, 0x1D, 0x1D];
const LINK: [u8; 3] = [0x60, 0xCD, 0xFF];

/// A rounded rectangle in the 48-unit design space.
#[derive(Clone, Copy)]
struct Shape {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    r: f32,
}

impl Shape {
    fn contains(&self, px: f32, py: f32) -> bool {
        if px < self.x || py < self.y || px > self.x + self.w || py > self.y + self.h {
            return false;
        }
        let cx = px.clamp(self.x + self.r, self.x + self.w - self.r);
        let cy = py.clamp(self.y + self.r, self.y + self.h - self.r);
        (px - cx).powi(2) + (py - cy).powi(2) <= self.r * self.r
    }
}

/// The tile's layers, back to front, in design units.
fn layers(edge: f32) -> Vec<(Shape, [u8; 3])> {
    let tile = Shape { x: 0.0, y: 0.0, w: 48.0, h: 48.0, r: 8.0 };
    let inner = Shape { x: edge, y: edge, w: 48.0 - 2.0 * edge, h: 48.0 - 2.0 * edge, r: 8.0 - edge };
    let mut out = vec![(tile, CARD_LINE), (inner, CARD)];
    for (i, h) in [10.0, 22.0, 15.0].into_iter().enumerate() {
        let bar = Shape { x: 13.5 + i as f32 * 8.0, y: 48.0 - 13.0 - h, w: 5.0, h, r: 1.0 };
        out.push((bar, LINK));
    }
    out
}

/// `size`² straight-alpha RGBA pixels, top row first, 4×4 samples a pixel.
fn draw(size: u32) -> Vec<[u8; 4]> {
    let s = size as f32 / 48.0;
    // The edge is one pixel wide at every size.
    let layers = layers(1.0 / s);
    let mut px = Vec::with_capacity((size * size) as usize);
    for y in 0..size {
        for x in 0..size {
            // Composite each layer's coverage over the ones below (premultiplied).
            let (mut r, mut g, mut b, mut a) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
            for (shape, c) in &layers {
                let mut hits = 0;
                for sy in 0..4 {
                    for sx in 0..4 {
                        let (ux, uy) =
                            ((x as f32 + (sx as f32 + 0.5) / 4.0) / s, (y as f32 + (sy as f32 + 0.5) / 4.0) / s);
                        hits += u32::from(shape.contains(ux, uy));
                    }
                }
                let cov = hits as f32 / 16.0;
                r = c[0] as f32 * cov + r * (1.0 - cov);
                g = c[1] as f32 * cov + g * (1.0 - cov);
                b = c[2] as f32 * cov + b * (1.0 - cov);
                a = cov + a * (1.0 - cov);
            }
            let un = |v: f32| if a > 0.0 { (v / a).round().clamp(0.0, 255.0) as u8 } else { 0 };
            px.push([un(r), un(g), un(b), (a * 255.0).round() as u8]);
        }
    }
    px
}

/// An `RT_ICON` image as a DIB: header (height doubled for the mask), BGRA rows bottom-up, an empty AND mask.
fn dib(size: u32, px: &[[u8; 4]]) -> Vec<u8> {
    let mask_row = size.div_ceil(32) * 4;
    let mut b = Vec::new();
    for v in [40, size, size * 2] {
        b.extend_from_slice(&u32::to_le_bytes(v));
    }
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&32u16.to_le_bytes());
    for v in [0, size * size * 4 + mask_row * size, 0, 0, 0, 0] {
        b.extend_from_slice(&u32::to_le_bytes(v));
    }
    for row in px.chunks(size as usize).rev() {
        row.iter().for_each(|p| b.extend_from_slice(&[p[2], p[1], p[0], p[3]]));
    }
    b.resize(b.len() + (mask_row * size) as usize, 0);
    b
}

/// Every size, as (size, `RT_ICON` data).
pub fn images() -> Vec<(u32, Vec<u8>)> {
    SIZES
        .iter()
        .map(|&size| {
            let px = draw(size);
            (size, if size >= 48 { png::encode(size, &px) } else { dib(size, &px) })
        })
        .collect()
}
