//! The read-only view our windows draw from.

use crate::history::History;
use crate::render::Gfx;
use crate::theme::Theme;
use busy_core::{Config, Snapshot};

pub struct Ctx<'a> {
    pub cfg: &'a Config,
    pub snap: &'a Snapshot,
    pub hist: &'a History,
    pub theme: &'a Theme,
    pub gfx: &'a Gfx,
}
