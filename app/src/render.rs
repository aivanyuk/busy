//! Shared Direct2D / DirectWrite helpers.

use crate::theme::{Color, alpha};
use busy_ui::history::Series;
use busy_win::utf16;
use windows::Win32::Graphics::Direct2D::Common::*;
use windows::Win32::Graphics::Direct2D::*;
use windows::Win32::Graphics::DirectWrite::*;
use windows::core::{BOOL, HSTRING, Interface, Result, w};

pub struct Gfx {
    pub d2d: ID2D1Factory,
    dw: IDWriteFactory,
    family: HSTRING,
    /// Tabular figures (OpenType `tnum`, design `font-variant-numeric: tabular-nums`): equal-width digits, so
    /// changing values don't jitter. Created once; applied to every layout, where it only affects digits.
    typography: IDWriteTypography,
    /// 3-on/3-off dashes for 1-DIP lines (CSS `1px dashed`), created once.
    dash: ID2D1StrokeStyle,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Left,
    Center,
    Right,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }
    pub fn right(&self) -> f32 {
        self.x + self.w
    }
    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && x < self.right() && y >= self.y && y < self.bottom()
    }
    pub fn inset(&self, dx: f32, dy: f32) -> Self {
        Self::new(self.x + dx, self.y + dy, (self.w - 2.0 * dx).max(0.0), (self.h - 2.0 * dy).max(0.0))
    }
    fn d2d(&self) -> D2D_RECT_F {
        D2D_RECT_F { left: self.x, top: self.y, right: self.right(), bottom: self.bottom() }
    }
}

impl Gfx {
    pub fn new() -> Result<Self> {
        unsafe {
            let d2d: ID2D1Factory = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
            let dw: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
            let mut coll = None;
            dw.GetSystemFontCollection(&mut coll, false)?;
            let has = |name: &HSTRING| {
                let (mut i, mut exists) = (0, BOOL(0));
                coll.as_ref().is_some_and(|c| c.FindFamilyName(name, &mut i, &mut exists).is_ok() && exists.as_bool())
            };
            let variable = HSTRING::from("Segoe UI Variable Text");
            let family = if has(&variable) { variable } else { HSTRING::from("Segoe UI") };
            let typography = dw.CreateTypography()?;
            typography.AddFontFeature(DWRITE_FONT_FEATURE {
                nameTag: DWRITE_FONT_FEATURE_TAG_TABULAR_FIGURES,
                parameter: 1,
            })?;
            let props = D2D1_STROKE_STYLE_PROPERTIES { dashStyle: D2D1_DASH_STYLE_CUSTOM, ..Default::default() };
            let dash = d2d.CreateStrokeStyle(&props, Some(&[3.0, 3.0]))?;
            Ok(Self { d2d, dw, family, typography, dash })
        }
    }

    /// Single-line, vertically centered, ellipsis-trimmed format. `size` in DIPs; `bold` is semibold (600).
    pub fn format(&self, size: f32, bold: bool) -> Result<IDWriteTextFormat> {
        self.format_weight(size, if bold { DWRITE_FONT_WEIGHT_SEMI_BOLD } else { DWRITE_FONT_WEIGHT_NORMAL })
    }

    /// `format` with any weight (the design's `font-weight: 700` is `DWRITE_FONT_WEIGHT_BOLD`).
    pub fn format_weight(&self, size: f32, weight: DWRITE_FONT_WEIGHT) -> Result<IDWriteTextFormat> {
        unsafe {
            let f = self.dw.CreateTextFormat(
                &self.family,
                None,
                weight,
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                size,
                w!("en-us"),
            )?;
            f.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;
            f.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
            let sign = self.dw.CreateEllipsisTrimmingSign(&f)?;
            let trim = DWRITE_TRIMMING { granularity: DWRITE_TRIMMING_GRANULARITY_CHARACTER, ..Default::default() };
            f.SetTrimming(&trim, &sign)?;
            Ok(f)
        }
    }

    /// Wrapping format for paragraphs (hints).
    pub fn wrapping(&self, size: f32) -> Result<IDWriteTextFormat> {
        let f = self.format(size, false)?;
        unsafe {
            f.SetWordWrapping(DWRITE_WORD_WRAPPING_WRAP)?;
            f.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_NEAR)?;
        }
        Ok(f)
    }

    pub fn text_width(&self, f: &IDWriteTextFormat, s: &str) -> f32 {
        self.metrics(f, s, 10_000.0).0
    }

    /// Width of `s` drawn with `tracking` DIPs after each character (CSS `letter-spacing`).
    pub fn text_width_tracked(&self, f: &IDWriteTextFormat, s: &str, tracking: f32) -> f32 {
        let Some(layout) = self.layout(f, s, 10_000.0, 10_000.0, tracking) else { return 0.0 };
        let mut m = DWRITE_TEXT_METRICS::default();
        // SAFETY: `m` is a valid out-pointer for the call.
        if unsafe { layout.GetMetrics(&mut m) }.is_err() {
            return 0.0;
        }
        m.widthIncludingTrailingWhitespace.ceil()
    }

    /// (width, height) of `s` laid out within `max_w`.
    pub fn metrics(&self, f: &IDWriteTextFormat, s: &str, max_w: f32) -> (f32, f32) {
        let Some(layout) = self.layout(f, s, max_w, 10_000.0, 0.0) else { return (0.0, 0.0) };
        let mut m = DWRITE_TEXT_METRICS::default();
        // SAFETY: `m` is a valid out-pointer for the call.
        if unsafe { layout.GetMetrics(&mut m) }.is_err() {
            return (0.0, 0.0);
        }
        (m.widthIncludingTrailingWhitespace.ceil(), m.height.ceil())
    }

    /// Text layout with tabular figures and `tracking` DIPs after each character; measuring and drawing text
    /// with digits or tracking both go through it, so widths match what is drawn.
    fn layout(
        &self,
        f: &IDWriteTextFormat,
        s: &str,
        max_w: f32,
        max_h: f32,
        tracking: f32,
    ) -> Option<IDWriteTextLayout> {
        let text = utf16(s);
        let all = DWRITE_TEXT_RANGE { startPosition: 0, length: text.len() as u32 };
        // SAFETY: `text` outlives the call (DirectWrite copies it); `f` and the typography are live COM objects.
        unsafe {
            let layout = self.dw.CreateTextLayout(&text, f, max_w, max_h).ok()?;
            let _ = layout.SetTypography(&self.typography, all);
            if tracking != 0.0
                && let Ok(l1) = layout.cast::<IDWriteTextLayout1>()
            {
                let _ = l1.SetCharacterSpacing(0.0, tracking, 0.0, all);
            }
            Some(layout)
        }
    }
}

pub struct Canvas<'a> {
    pub rt: &'a ID2D1RenderTarget,
    gfx: &'a Gfx,
    brush: ID2D1SolidColorBrush,
}

impl<'a> Canvas<'a> {
    pub fn new(rt: &'a ID2D1RenderTarget, gfx: &'a Gfx) -> Result<Self> {
        let brush = unsafe { rt.CreateSolidColorBrush(&Color::default(), None)? };
        Ok(Self { rt, gfx, brush })
    }

    fn brush(&self, c: Color) -> &ID2D1SolidColorBrush {
        unsafe { self.brush.SetColor(&c) };
        &self.brush
    }

    pub fn fill(&self, r: Rect, c: Color) {
        unsafe { self.rt.FillRectangle(&r.d2d(), self.brush(c)) }
    }

    pub fn round(&self, r: Rect, radius: f32, c: Color) {
        let rr = D2D1_ROUNDED_RECT { rect: r.d2d(), radiusX: radius, radiusY: radius };
        unsafe { self.rt.FillRoundedRectangle(&rr, self.brush(c)) }
    }

    /// 1-DIP outline of a rounded rect, drawn inside `r` (CSS `border: 1px solid`).
    pub fn round_outline(&self, r: Rect, radius: f32, c: Color) {
        let rr = D2D1_ROUNDED_RECT { rect: r.inset(0.5, 0.5).d2d(), radiusX: radius - 0.5, radiusY: radius - 0.5 };
        // SAFETY: the render target and brush are live COM objects.
        unsafe { self.rt.DrawRoundedRectangle(&rr, self.brush(c), 1.0, None) }
    }

    /// 1-DIP dashed horizontal line from `x0` to `x1` along the pixel row starting at `y`.
    pub fn dashed_hline(&self, x0: f32, x1: f32, y: f32, c: Color) {
        // SAFETY: the render target, brush and stroke style are live COM objects.
        unsafe { self.rt.DrawLine(point(x0, y + 0.5), point(x1, y + 0.5), self.brush(c), 1.0, &self.gfx.dash) }
    }

    /// 1-DIP vertical line centered on `x`.
    pub fn vline(&self, x: f32, y0: f32, y1: f32, c: Color) {
        self.fill(Rect::new(x - 0.5, y0, 1.0, y1 - y0), c);
    }

    pub fn text(&self, s: &str, f: &IDWriteTextFormat, r: Rect, c: Color, align: Align) {
        self.text_tracked(s, f, r, c, align, 0.0);
    }

    /// `text` with `tracking` DIPs after each character (CSS `letter-spacing`), as `Gfx::text_width_tracked`
    /// measures it.
    pub fn text_tracked(&self, s: &str, f: &IDWriteTextFormat, r: Rect, c: Color, align: Align, tracking: f32) {
        let a = match align {
            Align::Left => DWRITE_TEXT_ALIGNMENT_LEADING,
            Align::Center => DWRITE_TEXT_ALIGNMENT_CENTER,
            Align::Right => DWRITE_TEXT_ALIGNMENT_TRAILING,
        };
        // Typography needs a text layout, and `tnum` only changes digits: text without any (the constant labels)
        // is drawn as before, so only values pay for a layout per draw.
        if tracking == 0.0 && !s.bytes().any(|b| b.is_ascii_digit()) {
            // SAFETY: the format, render target and brush are live COM objects; the string outlives the call.
            unsafe {
                let _ = f.SetTextAlignment(a);
                self.rt.DrawText(
                    &utf16(s),
                    f,
                    &r.d2d(),
                    self.brush(c),
                    D2D1_DRAW_TEXT_OPTIONS_NONE,
                    DWRITE_MEASURING_MODE_NATURAL,
                );
            }
            return;
        }
        let Some(layout) = self.gfx.layout(f, s, r.w.max(0.0), r.h.max(0.0), tracking) else { return };
        // SAFETY: the layout, render target and brush are live COM objects.
        unsafe {
            let _ = layout.SetTextAlignment(a);
            self.rt.DrawTextLayout(point(r.x, r.y), &layout, self.brush(c), D2D1_DRAW_TEXT_OPTIONS_NONE);
        }
    }

    pub fn clip(&self, r: Rect) {
        unsafe { self.rt.PushAxisAlignedClip(&r.d2d(), D2D1_ANTIALIAS_MODE_ALIASED) }
    }

    pub fn unclip(&self) {
        unsafe { self.rt.PopAxisAlignedClip() }
    }

    /// Horizontal progress bar.
    pub fn hbar(&self, r: Rect, frac: f32, fg: Color, track: Color) {
        let rad = r.h / 2.0;
        self.round(r, rad, track);
        let w = (r.w * frac.clamp(0.0, 1.0)).max(if frac > 0.0 { r.h } else { 0.0 });
        if w > 0.0 {
            self.round(Rect { w, ..r }, rad, fg);
        }
    }

    /// Vertical bar filling from the bottom.
    pub fn vbar(&self, r: Rect, frac: f32, fg: Color, track: Color) {
        let rad = (r.w / 2.0).min(2.0);
        self.round(r, rad, track);
        let h = r.h * frac.clamp(0.0, 1.0);
        if h > 0.5 {
            self.round(Rect::new(r.x, r.bottom() - h, r.w, h), rad, fg);
        }
    }

    /// Area sparkline of `s`, right-aligned so the newest sample touches the right edge;
    /// the width spans the last `span` samples, so the graph fills up over time.
    /// `fill` is the opacity of the area under the line (0 = line only), `stroke` the line width.
    #[allow(clippy::too_many_arguments)]
    pub fn graph(&self, r: Rect, s: &Series, max: f32, c: Color, fill: f32, stroke: f32, span: usize) {
        let n = s.len();
        if n < 2 || max <= 0.0 {
            return;
        }
        let span = span.max(2);
        let first = n.saturating_sub(span);
        let step = r.w / (span - 1) as f32;
        let x = |i: usize| r.right() - (n - 1 - i) as f32 * step;
        let y = |i: usize| r.bottom() - (s.get(i) / max).clamp(0.0, 1.0) * r.h;
        let (Ok(area), Ok(line)) =
            (unsafe { self.gfx.d2d.CreatePathGeometry() }, unsafe { self.gfx.d2d.CreatePathGeometry() })
        else {
            return;
        };
        let (Ok(a), Ok(l)) = (unsafe { area.Open() }, unsafe { line.Open() }) else { return };
        unsafe {
            a.BeginFigure(point(x(first), r.bottom()), D2D1_FIGURE_BEGIN_FILLED);
            l.BeginFigure(point(x(first), y(first)), D2D1_FIGURE_BEGIN_HOLLOW);
            for i in first..n {
                a.AddLine(point(x(i), y(i)));
                if i > first {
                    l.AddLine(point(x(i), y(i)));
                }
            }
            a.AddLine(point(r.right(), r.bottom()));
            a.EndFigure(D2D1_FIGURE_END_CLOSED);
            l.EndFigure(D2D1_FIGURE_END_OPEN);
        }
        if unsafe { a.Close() }.is_err() || unsafe { l.Close() }.is_err() {
            return;
        }
        self.clip(r);
        if fill > 0.0 {
            unsafe { self.rt.FillGeometry(&area, self.brush(alpha(c, fill)), None) };
        }
        unsafe { self.rt.DrawGeometry(&line, self.brush(c), stroke, None) };
        self.unclip();
    }
}

/// Builds a `D2D_POINT_2F`. The `windows` crate types it as `windows_numerics::Vector2` (`#[repr(C)] { X: f32, Y: f32 }`)
/// without re-exporting that crate, and we don't take extra dependencies, so the value is built from its layout.
/// Only call where the parameter type is `D2D_POINT_2F`.
fn point<P: Copy>(x: f32, y: f32) -> P {
    const { assert!(size_of::<P>() == size_of::<[f32; 2]>()) };
    // SAFETY: P is `Vector2` at every call site; same size and layout as `[f32; 2]`.
    unsafe { std::mem::transmute_copy(&[x, y]) }
}

/// Nice upper bound for auto-scaled rate graphs (at least 1 KB/s).
pub fn nice_max(v: f32) -> f32 {
    let v = v.max(1024.0);
    let p = 2f32.powf(v.log2().ceil());
    if p * 0.75 >= v { p * 0.75 } else { p }
}

#[cfg(test)]
mod tests {
    #[test]
    fn fonts() {
        let g = super::Gfx::new().unwrap();
        let f = g.format(12.0, true).unwrap();
        g.wrapping(12.0).unwrap();
        // Tabular figures: every digit has the same advance (proportional: "1111" is narrower).
        assert_eq!(g.text_width(&f, "1111"), g.text_width(&f, "8888"));
        // Tracking adds its width after every character.
        let (plain, tracked) = (g.text_width(&f, "CPU"), g.text_width_tracked(&f, "CPU", 2.0));
        assert!((tracked - plain - 6.0).abs() <= 1.0, "{plain} {tracked}");
        g.format_weight(11.0, windows::Win32::Graphics::DirectWrite::DWRITE_FONT_WEIGHT_BOLD).unwrap();
    }
}
