//! The app icon, drawn in code so there are no binary assets to manage:
//! a soft filled circle with a small check mark.

pub fn rgba(size: u32) -> Vec<u8> {
    let s = size as f32;
    let (cx, cy, r) = (s / 2.0, s / 2.0, s / 2.0 - 0.5);
    let mut out = Vec::with_capacity((size * size * 4) as usize);
    // Check mark as two line segments, in unit coordinates.
    let segs = [((0.30, 0.52), (0.44, 0.66)), ((0.44, 0.66), (0.71, 0.37))];
    let stroke = s * 0.085;
    for y in 0..size {
        for x in 0..size {
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            let d = ((px - cx).powi(2) + (py - cy).powi(2)).sqrt();
            let circle = (r - d + 0.5).clamp(0.0, 1.0);
            let mut check: f32 = 0.0;
            for ((ax, ay), (bx, by)) in segs {
                let (ax, ay, bx, by) = (ax * s, ay * s, bx * s, by * s);
                let (vx, vy) = (bx - ax, by - ay);
                let t = (((px - ax) * vx + (py - ay) * vy) / (vx * vx + vy * vy)).clamp(0.0, 1.0);
                let dist = ((px - ax - t * vx).powi(2) + (py - ay - t * vy).powi(2)).sqrt();
                check = check.max((stroke - dist + 0.5).clamp(0.0, 1.0));
            }
            // Calm teal circle, white check.
            let (br, bg, bb) = (74.0, 140.0, 136.0);
            let mix = |base: f32| base + (255.0 - base) * check;
            out.extend_from_slice(&[
                mix(br) as u8,
                mix(bg) as u8,
                mix(bb) as u8,
                (circle * 255.0) as u8,
            ]);
        }
    }
    out
}
