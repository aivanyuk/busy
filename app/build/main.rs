//! Embeds the manifest, the version information and the icon in `busy.exe`. The manifest's version and the
//! `VERSIONINFO` come from `CARGO_PKG_VERSION`, the one place the version is written. The resources are
//! written as a `.res` file here (`res.rs`), with the icon drawn from the app glyph's geometry (`icon.rs`), so
//! no resource compiler and no binary asset are needed. It also writes the Microsoft Store package's layout to
//! `OUT_DIR/msix` (`msix.rs`), for `tools/pack-msix.ps1`.

use std::path::PathBuf;

mod icon;
mod msix;
mod png;
mod res;

fn main() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let out = PathBuf::from(std::env::var_os("OUT_DIR").unwrap_or_default());
    let version = env!("CARGO_PKG_VERSION");
    println!("cargo:rerun-if-changed=app.manifest");
    println!("cargo:rerun-if-changed=AppxManifest.xml");
    println!("cargo:rerun-if-changed=build");
    println!("cargo:rerun-if-changed=../LICENSE");
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() != Ok("msvc") {
        return;
    }
    let template = std::fs::read_to_string(dir.join("app.manifest")).unwrap_or_default();
    let manifest = out.join("app.manifest");
    let res = out.join("busy.res");
    // The copyright line of the license, as the file's "Copyright".
    let license = std::fs::read_to_string(dir.join("../LICENSE")).unwrap_or_default();
    let copyright = license.lines().find(|l| l.starts_with("Copyright")).unwrap_or("").to_string();
    let written = std::fs::write(&manifest, template.replace("@VERSION@", &res::four_part(version)))
        .and_then(|()| std::fs::write(&res, res::file(version, &copyright, &icon::images())));
    if let Err(e) = written {
        println!("cargo:warning=could not write the exe's resources: {e}");
        return;
    }
    let appx = std::fs::read_to_string(dir.join("AppxManifest.xml")).unwrap_or_default();
    if let Err(e) = msix::write(&out.join("msix"), &appx, &res::four_part(version)) {
        println!("cargo:warning=could not write the package layout: {e}");
    }
    println!("cargo:rustc-link-arg-bins=/MANIFEST:EMBED");
    println!("cargo:rustc-link-arg-bins=/MANIFESTINPUT:{}", manifest.display());
    // link.exe takes a compiled resource file as an input like an object file.
    println!("cargo:rustc-link-arg-bins={}", res.display());
    // Static imports resolve from System32 only (LOAD_LIBRARY_SEARCH_SYSTEM32), never the exe's directory.
    println!("cargo:rustc-link-arg-bins=/DEPENDENTLOADFLAG:0x800");
}
