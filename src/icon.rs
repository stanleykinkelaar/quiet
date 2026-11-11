//! The app icon, drawn in code so there are no binary assets to manage:
//! a cream crescent moon on a night blue tile, cradling a mint check mark,
//! with a few small stars.
//!
//! Every shape is a signed distance in unit coordinates (0 to 1 across the
//! icon). Each pixel averages a grid of samples, which antialiases edges at
//! any size.

/// Premultiplied RGBA, 0 to 1.
type Color = [f32; 4];

pub fn rgba(size: u32) -> Vec<u8> {
    // More samples for small sizes, where every edge pixel shows.
    let n = if size <= 64 { 8 } else { 4 };
    let step = 1.0 / n as f32;
    let mut out = Vec::with_capacity((size * size * 4) as usize);
    for py in 0..size {
        for px in 0..size {
            let mut acc = [0.0f32; 4];
            for sy in 0..n {
                for sx in 0..n {
                    let x = (px as f32 + (sx as f32 + 0.5) * step) / size as f32;
                    let y = (py as f32 + (sy as f32 + 0.5) * step) / size as f32;
                    let c = shade(x, y);
                    (0..4).for_each(|i| acc[i] += c[i]);
                }
            }
            let a = acc[3] / (n * n) as f32;
            let channel = |v: f32| if a > 0.0 { (v / (n * n) as f32 / a * 255.0).round().clamp(0.0, 255.0) as u8 } else { 0 };
            out.extend_from_slice(&[channel(acc[0]), channel(acc[1]), channel(acc[2]), (a * 255.0).round() as u8]);
        }
    }
    out
}

fn shade(x: f32, y: f32) -> Color {
    if rounded_square(x, y, 0.45, 0.2) > 0.0 {
        return [0.0; 4];
    }
    // Night sky, lighter towards the bottom right, with a soft glow behind the moon.
    let mut c = mix(rgb(0x1d2747), rgb(0x2f5573), (x * 0.4 + y * 0.6 - 0.1) / 0.8);
    let glow = (1.0 - length(x - 0.47, y - 0.5) / 0.42).clamp(0.0, 1.0);
    c = over(c, alpha(rgb(0x9fd8ff), 0.18 * glow * glow));

    for (sx, sy, r) in [(0.74, 0.25, 0.012), (0.80, 0.36, 0.008), (0.66, 0.19, 0.007)] {
        if circle(x, y, sx, sy, r) < 0.0 {
            c = over(c, alpha(rgb(0xfff6e0), 0.85));
        }
    }

    // The crescent: a disc with a smaller disc cut out of its upper right.
    let moon = circle(x, y, 0.45, 0.52, 0.29).max(-circle(x, y, 0.58, 0.44, 0.24));
    if moon < 0.0 {
        c = over(c, mix(rgb(0xfff3da), rgb(0xf2d9ad), (y - 0.25) / 0.55));
    }

    // The check sits entirely inside the cut out disc, so it never touches the moon.
    let check = segment(x, y, (0.48, 0.47), (0.56, 0.55)).min(segment(x, y, (0.56, 0.55), (0.71, 0.38)));
    if check < 0.04 {
        c = over(c, rgb(0x8fe3c4));
    }
    c
}

fn rgb(hex: u32) -> Color {
    let channel = |shift: u32| ((hex >> shift) & 0xff) as f32 / 255.0;
    [channel(16), channel(8), channel(0), 1.0]
}

fn alpha(c: Color, a: f32) -> Color {
    c.map(|v| v * a)
}

fn mix(a: Color, b: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    [0, 1, 2, 3].map(|i| a[i] + (b[i] - a[i]) * t)
}

/// `src` drawn over `dst`, both premultiplied.
fn over(dst: Color, src: Color) -> Color {
    [0, 1, 2, 3].map(|i| src[i] + dst[i] * (1.0 - src[3]))
}

fn length(x: f32, y: f32) -> f32 {
    (x * x + y * y).sqrt()
}

/// A square centred in the icon, `half` wide each way, corners of radius `r`.
fn rounded_square(x: f32, y: f32, half: f32, r: f32) -> f32 {
    let qx = (x - 0.5).abs() - (half - r);
    let qy = (y - 0.5).abs() - (half - r);
    length(qx.max(0.0), qy.max(0.0)) + qx.max(qy).min(0.0) - r
}

fn circle(x: f32, y: f32, cx: f32, cy: f32, r: f32) -> f32 {
    length(x - cx, y - cy) - r
}

/// Distance to the line segment from `a` to `b`.
fn segment(x: f32, y: f32, a: (f32, f32), b: (f32, f32)) -> f32 {
    let (vx, vy) = (b.0 - a.0, b.1 - a.1);
    let t = (((x - a.0) * vx + (y - a.1) * vy) / (vx * vx + vy * vy)).clamp(0.0, 1.0);
    length(x - a.0 - t * vx, y - a.1 - t * vy)
}
