//! One file per module: what its flyout shows (design `fly()`), as a `Detail`.

mod battery;
mod cpu;
mod disk;
mod gpu;
mod memory;
mod network;
mod sensors;

use super::detail::Detail;
use crate::ctx::Ctx;
use busy_core::{Module, ModuleCfg, ProcEntry, TOP_N};

/// The module's flyout content.
pub(super) fn detail<'a>(ctx: &Ctx<'a>, mc: &ModuleCfg) -> Detail<'a> {
    match mc.module {
        Module::Cpu => cpu::detail(ctx, mc),
        Module::Memory => memory::detail(ctx, mc),
        Module::Gpu => gpu::detail(ctx, mc),
        Module::Disk => disk::detail(ctx, mc),
        Module::Network => network::detail(ctx, mc),
        Module::Battery => battery::detail(ctx, mc),
        Module::Sensors => sensors::detail(ctx, mc),
        // No cell, so never opened; its lists are part of the CPU, Memory and Disk flyouts.
        Module::Processes => Detail::new("Processes"),
    }
}

/// Top-process rows from `list`, unless Processes' `flyout` turned the lists off.
fn procs(ctx: &Ctx, list: &[ProcEntry], value: impl Fn(&ProcEntry) -> String) -> Vec<(String, String)> {
    if !ctx.cfg.module(Module::Processes).is_some_and(|c| c.flyout) {
        return Vec::new();
    }
    list.iter().take(TOP_N).map(|p| (p.name.clone(), value(p))).collect()
}
