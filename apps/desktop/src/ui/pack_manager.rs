//! The sticker manager dialog: the packs on the left (the user's own, the room's, those used in
//! all their rooms, and a new pack where the room allows it), the one picked on the right with
//! its stickers. A pack the user may change is renamed, added to from files, and each sticker
//! opened to change its name, description and use, made the pack's picture or removed.

use rune::{Color, El, ScrollSpec, col, el, icon, row, text};

use super::components::{ButtonKind, button, check_row, closable_dialog, field_input, labelled, switch_row};
use super::icons;
use super::stickers::pack_name;
use super::theme::{Palette, font};
use crate::i18n::t;
use crate::state::{ImageForm, PackField, PackManager, State};
use matrix_core::{ImagePack, PackPlace};

/// The side of a sticker in the manager's grid.
const TILE: f32 = 76.0;

pub fn dialog_for(s: &State, p: &Palette) -> Option<El> {
    let manager = s.pack_manager.as_ref()?;
    let packs = s.managed_packs();
    let content = row()
        .w_full()
        .h(520.0)
        .gap(16.0)
        .child(pack_list(s, manager, &packs, p))
        .child(el().w(1.0).h_full().fill(p.divider))
        .child(
            col()
                .key("pack-detail")
                .grow(1.0)
                .min_w(0.0)
                .h_full()
                .scroll(ScrollSpec { stick_bottom: false })
                .child(detail(s, manager, p)),
        );
    Some(closable_dialog("pack-manager", t!("packs.title"), content, Vec::new(), p).w(760.0))
}

fn pack_list(s: &State, manager: &PackManager, packs: &[&ImagePack], p: &Palette) -> El {
    let row_for = |pack: &ImagePack| {
        let selected = pack.id == manager.selected;
        let picture = pack.avatar.as_ref().or_else(|| pack.images.first().map(|image| &image.media));
        let face = el().size(28.0, 28.0).radius(6.0);
        let face = match picture.and_then(|media| s.media.image(media)) {
            Some(image) => face.pixels_fit(image.pixels(), rune::SurfaceFit::Fit),
            None => face.fill(p.hover).center().child(icon(&icons::STICKER, 16.0).color(p.text_muted)),
        };
        let id = pack.id.clone();
        row()
            .key(format!("pack-{}", pack.id))
            .w_full()
            .px(8.0)
            .py(6.0)
            .gap(10.0)
            .center_y()
            .radius(8.0)
            .fill(if selected { p.accent.alpha(0.12) } else { Color::TRANSPARENT })
            .hover(p.hover)
            .role("tab")
            .checked(selected)
            .focusable(true)
            .cursor(rune::Cursor::Pointer)
            .label(pack_name(pack))
            .child(face)
            .child(text(pack_name(pack)).font(font::BODY).color(p.text).grow(1.0).min_w(0.0).truncate())
            .on_press(move |s: &mut State| s.select_pack(&id))
    };
    let heading = |title: &str| text(title.to_owned()).font(font::SECTION).color(p.text_muted).px(8.0).pt(10.0).pb(2.0);
    let here = |pack: &&&ImagePack| match (&pack.place, &manager.room_id) {
        (PackPlace::Room { room_id, .. }, Some(room)) => room_id == room,
        _ => false,
    };
    let mut list = col().w(200.0).h_full().gap(2.0).scroll(ScrollSpec { stick_bottom: false }).key("pack-list");
    list = list.children(packs.iter().filter(|pack| pack.place == PackPlace::User).map(|pack| row_for(pack)));
    let room_packs: Vec<&&ImagePack> = packs.iter().filter(here).collect();
    if !room_packs.is_empty() || s.may_add_managed_pack() {
        list = list.child(heading(t!("packs.this_room"))).children(room_packs.iter().map(|pack| row_for(pack)));
    }
    if s.may_add_managed_pack() {
        list = list.child(match &manager.new_pack {
            Some(name) => col()
                .w_full()
                .gap(6.0)
                .px(4.0)
                .pt(4.0)
                .child(
                    field_input(name, t!("packs.new_pack_name"), p)
                        .key("pack-new-name")
                        .on_edit(|s: &mut State| s.pack_field(PackField::NewPack))
                        .on_submit(|s: &mut State, _| s.create_pack()),
                )
                .child(
                    button(t!("packs.create"), ButtonKind::Primary, !name.text.trim().is_empty(), p)
                        .key("pack-create")
                        .on_press(State::create_pack),
                ),
            None => row()
                .key("pack-new")
                .w_full()
                .px(8.0)
                .py(6.0)
                .gap(10.0)
                .center_y()
                .radius(8.0)
                .hover(p.hover)
                .role("button")
                .cursor(rune::Cursor::Pointer)
                .label(t!("packs.new_pack"))
                .child(icon(&icons::PLUS, 16.0).color(p.accent))
                .child(text(t!("packs.new_pack")).font(font::BODY).color(p.accent))
                .on_press(State::start_new_pack),
        });
    }
    let elsewhere: Vec<&&ImagePack> =
        packs.iter().filter(|pack| pack.place != PackPlace::User && !here(pack)).collect();
    if !elsewhere.is_empty() {
        list = list.child(heading(t!("packs.everywhere"))).children(elsewhere.iter().map(|pack| row_for(pack)));
    }
    list
}

fn detail(s: &State, manager: &PackManager, p: &Palette) -> El {
    let Some(pack) = s.managed_pack() else {
        return text(t!("packs.loading")).font(font::BODY).color(p.text_muted).py(12.0);
    };
    let busy = manager.busy.is_some();
    let mut out = col().w_full().gap(14.0).pr(8.0);
    // The name: a field and Rename where the user may change it.
    if pack.editable {
        let name = field_input(&manager.name, t!("packs.name_placeholder"), p)
            .key("pack-name")
            .grow(1.0)
            .on_edit(|s: &mut State| s.pack_field(PackField::Name))
            .on_submit(|s: &mut State, _| s.rename_pack());
        let changed = manager.name.text.trim() != pack.name;
        out = out.child(labelled(
            t!("packs.name"),
            row().w_full().gap(8.0).center_y().child(name).child(
                button(t!("packs.rename"), ButtonKind::Secondary, changed, p)
                    .key("pack-rename")
                    .on_press(State::rename_pack),
            ),
            p,
        ));
    } else {
        out = out
            .child(text(pack_name(pack)).font(font::HEADER).color(p.text))
            .child(text(t!("packs.read_only")).font(font::META).color(p.text_muted).wrap());
    }
    if matches!(pack.place, PackPlace::Room { .. }) {
        let on = pack.everywhere;
        out = out.child(
            switch_row(t!("packs.use_everywhere"), Some(t!("packs.use_everywhere_hint")), on, p)
                .key("pack-everywhere")
                .on_press(move |s: &mut State| s.use_pack_everywhere(!on)),
        );
    }
    if let Some(form) = &manager.image {
        return out.child(image_form(s, pack, form, p));
    }
    if pack.editable {
        let mut actions = row().w_full().gap(8.0).center_y().child(
            button(t!("packs.add_stickers"), ButtonKind::Primary, !busy, p)
                .key("pack-add")
                .on_press(State::pick_pack_images),
        );
        if busy {
            actions = actions.child(text(t!("packs.saving")).font(font::META).color(p.text_muted));
        }
        out = out.child(actions);
    }
    if pack.images.is_empty() {
        out = out.child(text(t!("packs.empty")).font(font::BODY).color(p.text_muted).wrap());
    }
    let mut grid = row().w_full().flex_wrap().gap(6.0);
    for image in &pack.images {
        let picture = el().size(TILE - 12.0, TILE - 12.0);
        let picture = match s.media.image(&image.media) {
            Some(loaded) => picture.pixels_fit(loaded.pixels(), rune::SurfaceFit::Fit),
            None => picture.radius(8.0).fill(p.hover),
        };
        let mut tile = col()
            .key(format!("pack-image-{}", image.shortcode))
            .w(TILE)
            .gap(2.0)
            .items_center()
            .radius(8.0)
            .py(4.0)
            .label(image.body.clone())
            .help(format!(":{}:", image.shortcode))
            .child(picture)
            .child(text(image.shortcode.clone()).font(font::META).color(p.text_muted).w(TILE - 8.0).truncate());
        if pack.editable {
            let code = image.shortcode.clone();
            tile = tile
                .hover(p.hover.alpha(p.hover.a * 2.0))
                .role("button")
                .cursor(rune::Cursor::Pointer)
                .on_press(move |s: &mut State| s.open_pack_image(&code));
        }
        grid = grid.child(tile);
    }
    out = out.child(grid);
    if pack.editable && matches!(pack.place, PackPlace::Room { .. }) {
        let label = if manager.deleting { t!("packs.delete_confirm") } else { t!("packs.delete") };
        out = out.child(
            row()
                .w_full()
                .pt(8.0)
                .child(button(label, ButtonKind::Danger, !busy, p).key("pack-delete").on_press(State::delete_pack)),
        );
    }
    out
}

/// One sticker's details being changed.
fn image_form(s: &State, pack: &ImagePack, form: &ImageForm, p: &Palette) -> El {
    let image = pack.images.iter().find(|image| image.shortcode == form.shortcode);
    let picture = el().size(96.0, 96.0);
    let picture = match image.and_then(|image| s.media.image(&image.media)) {
        Some(loaded) => picture.pixels_fit(loaded.pixels(), rune::SurfaceFit::Fit),
        None => picture.radius(8.0).fill(p.hover),
    };
    let field =
        |edit: &rune::TextEdit, key: &str, placeholder: &str| field_input(edit, placeholder, p).key(key.to_owned());
    let code = field(&form.code, "pack-image-code", t!("packs.shortcode_placeholder"))
        .on_edit(|s: &mut State| s.pack_field(PackField::Shortcode));
    let body = field(&form.body, "pack-image-body", t!("packs.description_placeholder"))
        .on_edit(|s: &mut State| s.pack_field(PackField::Description))
        .on_submit(|s: &mut State, _| s.save_pack_image());
    let is_avatar = image.is_some_and(|image| pack.avatar_url.as_deref() == Some(image.url.as_str()));
    col()
        .key("pack-image-form")
        .w_full()
        .gap(12.0)
        .padding(12.0)
        .radius(10.0)
        .border(1.0, p.divider)
        .child(row().w_full().gap(16.0).child(picture).child(
            col().grow(1.0).min_w(0.0).gap(10.0).child(labelled(t!("packs.shortcode"), code, p)).child(labelled(
                t!("packs.description"),
                body,
                p,
            )),
        ))
        .child(
            check_row(t!("packs.as_sticker"), Some(t!("packs.as_sticker_hint")), form.sticker, p)
                .key("pack-image-sticker")
                .on_press(|s: &mut State| s.toggle_pack_image_use(true)),
        )
        .child(
            check_row(t!("packs.as_emoji"), Some(t!("packs.as_emoji_hint")), form.emoticon, p)
                .key("pack-image-emoji")
                .on_press(|s: &mut State| s.toggle_pack_image_use(false)),
        )
        .child(
            // In a narrow window Cancel and Save wrap onto their own line rather than leave it.
            row()
                .w_full()
                .gap(8.0)
                .center_y()
                .flex_wrap()
                .child(
                    button(t!("packs.remove"), ButtonKind::Danger, true, p)
                        .key("pack-image-remove")
                        .on_press(State::remove_pack_image),
                )
                .child(
                    button(t!("packs.use_as_picture"), ButtonKind::Secondary, !is_avatar, p)
                        .key("pack-image-avatar")
                        .on_press(State::use_as_pack_picture),
                )
                .child(el().grow(1.0))
                .child(
                    button(t!("packs.cancel"), ButtonKind::Secondary, true, p)
                        .key("pack-image-cancel")
                        .on_press(State::close_pack_image),
                )
                .child(
                    button(t!("packs.save"), ButtonKind::Primary, true, p)
                        .key("pack-image-save")
                        .on_press(State::save_pack_image),
                ),
        )
}
