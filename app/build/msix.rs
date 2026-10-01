//! The Microsoft Store package's layout (docs/plans/store.md): `AppxManifest.xml` from its template, with the
//! version filled in, and the logos, drawn from the app glyph like the exe's icon. `tools/pack-msix.ps1` adds
//! the exe and the license files, indexes the logos (`resources.pri`) and packs it.

use crate::{icon, png};
use std::path::Path;

/// Taskbar, Start's list and search ask for these exact sizes; `altform-unplated` is the variant shown without
/// the accent plate, which the tile, with its own background, is meant for.
const TARGET_SIZES: [u32; 9] = [16, 20, 24, 32, 40, 48, 64, 96, 256];
/// Display scales, in percent.
const SCALES: [u32; 5] = [100, 125, 150, 200, 400];

/// (file name under `Assets`, PNG) for every logo the manifest names, at every size Windows picks from.
fn logos() -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    let mut add =
        |name: String, canvas: u32, size: u32| out.push((name, png::encode(canvas, &icon::draw_in(canvas, size))));
    for t in TARGET_SIZES {
        add(format!("Square44x44Logo.targetsize-{t}.png"), t, t);
        add(format!("Square44x44Logo.targetsize-{t}_altform-unplated.png"), t, t);
    }
    for s in SCALES {
        let px = |base: u32| (base * s).div_ceil(100);
        add(format!("Square44x44Logo.scale-{s}.png"), px(44), px(44));
        // Start's medium tile: the glyph's tile at half the square, as Windows' own apps leave room around theirs.
        add(format!("Square150x150Logo.scale-{s}.png"), px(150), px(75));
        add(format!("StoreLogo.scale-{s}.png"), px(50), px(50));
    }
    out
}

/// Writes the layout under `dir`.
pub fn write(dir: &Path, template: &str, version: &str) -> std::io::Result<()> {
    let assets = dir.join("Assets");
    std::fs::create_dir_all(&assets)?;
    std::fs::write(dir.join("AppxManifest.xml"), template.replace("@VERSION@", version))?;
    for (name, data) in logos() {
        std::fs::write(assets.join(name), data)?;
    }
    Ok(())
}
