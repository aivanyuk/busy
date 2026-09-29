//! The 32-bpp DIB the widget is drawn into before `UpdateLayeredWindow`.

#[cfg(debug_assertions)]
use crate::theme::Color;
use windows::Win32::Graphics::Gdi::*;

/// 32-bpp top-down DIB selected into a memory DC.
pub(super) struct Surface {
    pub(super) dc: HDC,
    bmp: HBITMAP,
    old: HGDIOBJ,
    pub(super) w: i32,
    pub(super) h: i32,
    #[cfg_attr(not(debug_assertions), allow(dead_code))]
    bits: *mut u8,
}

impl Surface {
    pub(super) fn new(w: i32, h: i32) -> Option<Self> {
        unsafe {
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
                return None;
            };
            let old = SelectObject(dc, bmp.into());
            Some(Self { dc, bmp, old, w, h, bits: bits as _ })
        }
    }
}

impl Drop for Surface {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.dc, self.old);
            let _ = DeleteObject(self.bmp.into());
            let _ = DeleteDC(self.dc);
        }
    }
}

/// Debug builds: `BUSY_DUMP=<dir>` writes the widget (composited over the `--tb` color) to `taskbar.bmp`.
#[cfg(debug_assertions)]
pub(super) fn debug_dump(s: &Surface, tb: Color) {
    let Some(dir) = std::env::var_os("BUSY_DUMP") else { return };
    let (w, h) = (s.w as usize, s.h as usize);
    let px = unsafe { std::slice::from_raw_parts(s.bits, w * h * 4) };
    // Opaque `--tb`: close to what the widget sits on.
    let bg = [tb.b, tb.g, tb.r].map(|c| c * 255.0);
    let mut out = Vec::with_capacity(54 + w * h * 4);
    let file_len = (54 + w * h * 4) as u32;
    out.extend_from_slice(b"BM");
    out.extend_from_slice(&file_len.to_le_bytes());
    out.extend_from_slice(&[0, 0, 0, 0, 54, 0, 0, 0, 40, 0, 0, 0]);
    out.extend_from_slice(&(w as i32).to_le_bytes());
    out.extend_from_slice(&(-(h as i32)).to_le_bytes());
    out.extend_from_slice(&[1, 0, 32, 0]);
    out.extend_from_slice(&[0; 24]);
    for p in px.as_chunks::<4>().0 {
        let a = p[3] as f32 / 255.0;
        for c in 0..3 {
            out.push((p[c] as f32 + bg[c] * (1.0 - a)).min(255.0) as u8);
        }
        out.push(255);
    }
    // Hard rule: no file I/O on the UI thread, not even in debug builds.
    std::thread::spawn(move || std::fs::write(std::path::Path::new(&dir).join("taskbar.bmp"), out));
}
