//! Debug builds: `BUSY_DUMP=<dir>` also draws each frame into a DIB and writes it to `settings.bmp`, so the
//! window can be checked without a screen capture (e.g. while the session is locked).

use busy_ui::render::{Canvas, Gfx};
use windows::Win32::Foundation::RECT;
use windows::Win32::Graphics::Direct2D::Common::*;
use windows::Win32::Graphics::Direct2D::*;
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::Graphics::Gdi::*;

pub(super) fn frame(gfx: &Gfx, (w, h): (i32, i32), dpi: u32, draw: impl FnOnce(&Canvas)) {
    let Some(dir) = std::env::var_os("BUSY_DUMP") else { return };
    if w <= 0 || h <= 0 {
        return;
    }
    // SAFETY: GDI and D2D objects created here are used and released within this function; `bits` points to
    // the DIB's w·h·4 bytes while the bitmap lives.
    let out = unsafe {
        let dc = CreateCompatibleDC(None);
        let bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits = std::ptr::null_mut();
        let Ok(bmp) = CreateDIBSection(Some(dc), &bmi, DIB_RGB_COLORS, &mut bits, None, 0) else {
            let _ = DeleteDC(dc);
            return;
        };
        let old = SelectObject(dc, bmp.into());
        let props = D2D1_RENDER_TARGET_PROPERTIES {
            pixelFormat: D2D1_PIXEL_FORMAT { format: DXGI_FORMAT_B8G8R8A8_UNORM, alphaMode: D2D1_ALPHA_MODE_IGNORE },
            dpiX: dpi as f32,
            dpiY: dpi as f32,
            ..Default::default()
        };
        let mut out = None;
        if let Ok(rt) = gfx.d2d.CreateDCRenderTarget(&props)
            && rt.BindDC(dc, &RECT { left: 0, top: 0, right: w, bottom: h }).is_ok()
        {
            rt.BeginDraw();
            if let Ok(cv) = Canvas::new(&rt, gfx) {
                draw(&cv);
            }
            if rt.EndDraw(None, None).is_ok() {
                out = Some(bmp_file(std::slice::from_raw_parts(bits as *const u8, (w * h * 4) as usize), w, h));
            }
        }
        SelectObject(dc, old);
        let _ = DeleteObject(bmp.into());
        let _ = DeleteDC(dc);
        out
    };
    if let Some(out) = out {
        // No file I/O on the UI thread, not even in debug builds.
        std::thread::spawn(move || std::fs::write(std::path::Path::new(&dir).join("settings.bmp"), out));
    }
}

fn bmp_file(px: &[u8], w: i32, h: i32) -> Vec<u8> {
    let mut out = Vec::with_capacity(54 + px.len());
    out.extend_from_slice(b"BM");
    out.extend_from_slice(&((54 + px.len()) as u32).to_le_bytes());
    out.extend_from_slice(&[0, 0, 0, 0, 54, 0, 0, 0, 40, 0, 0, 0]);
    out.extend_from_slice(&w.to_le_bytes());
    out.extend_from_slice(&(-h).to_le_bytes());
    out.extend_from_slice(&[1, 0, 32, 0]);
    out.extend_from_slice(&[0; 24]);
    out.extend_from_slice(px);
    out
}
