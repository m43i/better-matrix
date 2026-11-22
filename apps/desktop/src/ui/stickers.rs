//! The sticker picker: the room's image packs' stickers in a grid, each pack under its name; a
//! press sends one. Anchored to the composer's sticker button.

use rune::{Align, El, ScrollSpec, Side, col, el, row, text};

use super::components::popover_panel;
use super::composer::sticker_button;
use super::theme::{Palette, font};
use crate::i18n::t;
use crate::state::{ComposerTarget, State};

/// The side of a sticker in the grid.
const STICKER: f32 = 72.0;

pub fn picker(s: &State, target: ComposerTarget, p: &Palette) -> El {
    let packs = match (s.nav.account.as_ref(), s.active_room_id()) {
        (Some(account), Some(room_id)) => s.sticker_packs(account, room_id),
        _ => Vec::new(),
    };
    let mut grid = col().w_full().px(8.0).pb(8.0).gap(6.0);
    if packs.is_empty() {
        grid = grid.child(text(t!("stickers.none")).font(font::BODY).color(p.text_muted).px(4.0).py(12.0).wrap());
    }
    for (pack, stickers) in packs {
        let mut set = row().w_full().flex_wrap().gap(4.0);
        for image in stickers {
            let picture = el().size(STICKER - 8.0, STICKER - 8.0);
            let picture = match s.media.image(&image.media) {
                Some(loaded) => picture.pixels_fit(loaded.pixels(), rune::SurfaceFit::Fit),
                None => picture.radius(8.0).fill(p.hover),
            };
            let sent = image.clone();
            set = set.child(
                el().key(format!("sticker-{}-{}", pack.id, image.shortcode))
                    .size(STICKER, STICKER)
                    .radius(8.0)
                    .center()
                    .hover(p.hover.alpha(p.hover.a * 3.0))
                    .role("button")
                    .label(image.body.clone())
                    .help(image.body.clone())
                    .cursor(rune::Cursor::Pointer)
                    .child(picture)
                    .on_press_tx(move |s: &mut State, tx| s.send_sticker(target, sent.clone(), tx)),
            );
        }
        grid = grid.child(text(pack.name.clone()).font(font::SECTION).color(p.text_muted).px(4.0).pt(4.0)).child(set);
    }
    let panel = popover_panel(p)
        .key("sticker-picker")
        .w(344.0)
        .preserve_focus_on_press(true)
        .child(text(t!("composer.stickers")).font(font::SECTION).color(p.text).px(12.0).py(6.0))
        .child(col().key("sticker-grid").w_full().max_h(320.0).scroll(ScrollSpec { stick_bottom: false }).child(grid));
    col().child(panel).anchored(sticker_button(target), Side::Top, Align::Start, 6.0)
}
