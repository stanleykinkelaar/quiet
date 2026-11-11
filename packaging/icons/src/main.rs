//! Writes the app icon (drawn by `src/icon.rs`) as PNG files:
//!
//! * `<out>/quiet.iconset/`, which `iconutil -c icns` turns into a macOS icon
//! * `<out>/hicolor/<size>x<size>/apps/quiet.png`, the Linux icon theme layout
//!
//! Usage: `cargo run -p quiet-icons -- <out>`

use std::path::{Path, PathBuf};

#[path = "../../../src/icon.rs"]
mod icon;

fn write_png(path: &Path, size: u32) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    image::RgbaImage::from_raw(size, size, icon::rgba(size))
        .unwrap()
        .save(path)
        .unwrap_or_else(|err| panic!("writing {}: {err}", path.display()));
}

fn main() {
    let out = PathBuf::from(std::env::args().nth(1).expect("usage: quiet-icons <out dir>"));

    let iconset = out.join("quiet.iconset");
    for size in [16, 32, 128, 256, 512] {
        write_png(&iconset.join(format!("icon_{size}x{size}.png")), size);
        write_png(&iconset.join(format!("icon_{size}x{size}@2x.png")), size * 2);
    }

    for size in [16, 22, 24, 32, 48, 64, 128, 256, 512] {
        write_png(&out.join(format!("hicolor/{size}x{size}/apps/quiet.png")), size);
    }
}
