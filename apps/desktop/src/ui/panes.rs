//! The shared resize handle on a pane's inner edge.

use rune::{Color, Cursor, El, el};

use super::theme::Palette;
use crate::i18n::t;
use crate::state::{Pane, PaneSize, State};

#[cfg(test)]
mod tests;

pub fn resizable(content: El, pane: Pane, size: PaneSize, p: &Palette) -> El {
    let (key, label) = match pane {
        Pane::Sidebar => ("sidebar-resize", t!("sidebar.resize")),
        Pane::SidePanel => ("side-panel-resize", t!("info.resize")),
        Pane::Thread => ("thread-resize", t!("thread.resize")),
    };
    let sidebar = pane == Pane::Sidebar;
    let handle = el()
        .key(key)
        .pin(if sidebar { None } else { Some(0.0) }, Some(0.0), if sidebar { Some(0.0) } else { None }, Some(0.0))
        .w(6.0)
        .fill(Color::TRANSPARENT)
        .hover(p.accent.alpha(0.4))
        .cursor(Cursor::ResizeColumn)
        .role("separator")
        .label(label)
        .range_value(f64::from(size.width), f64::from(size.min), f64::from(size.max))
        .range_step(10.0)
        .on_drag(move |s: &mut State, drag, _| s.panes.drag(pane, size, drag))
        .on_step(move |s: &mut State, delta, tx| {
            s.panes.set(pane, size.clamp(size.width + delta as f32 * 10.0));
            tx.instant();
        })
        .on_step_limit(move |s: &mut State, max, tx| {
            s.panes.set(pane, if max { size.max } else { size.min });
            tx.instant();
        });
    el().key(format!("{key}-pane")).w(size.width).h_full().child(content.min_w(0.0)).child(handle)
}
