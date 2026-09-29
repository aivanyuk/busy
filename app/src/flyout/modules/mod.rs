//! One file per module: what its flyout shows (design `fly()`), as a `Detail`.

mod cpu;
mod disk;
mod gpu;
mod memory;
mod network;

use super::detail::Detail;
use crate::ctx::Ctx;
use busy_core::{Module, ModuleCfg, ProcEntry, TOP_N};

/// The module's flyout content; `None` for a module still drawn by the older sections.
pub(super) fn detail<'a>(ctx: &Ctx<'a>, mc: &ModuleCfg) -> Option<Detail<'a>> {
    match mc.module {
        Module::Cpu => Some(cpu::detail(ctx, mc)),
        Module::Memory => Some(memory::detail(ctx, mc)),
        Module::Gpu => Some(gpu::detail(ctx, mc)),
        Module::Disk => Some(disk::detail(ctx, mc)),
        Module::Network => Some(network::detail(ctx, mc)),
        _ => None,
    }
}

/// Top-process rows from `list`, unless Processes' `flyout` turned the lists off.
fn procs(ctx: &Ctx, list: &[ProcEntry], value: impl Fn(&ProcEntry) -> String) -> Vec<(String, String)> {
    if !ctx.cfg.module(Module::Processes).is_some_and(|c| c.flyout) {
        return Vec::new();
    }
    list.iter().take(TOP_N).map(|p| (p.name.clone(), value(p))).collect()
}
