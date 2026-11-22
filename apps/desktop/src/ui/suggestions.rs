//! The list of suggestions over a composer while a mention, room, emoji or command is typed
//! (`state::suggest`): people and rooms with their pictures, emoji with their short codes,
//! commands with what they take and do. Up and Down move, Tab or Enter take, Escape hides it;
//! a press takes one. It sits on the composer's frame and leaves the field the keyboard.

use rune::{Align, El, Side, col, el, row, text};

use super::components::media_avatar;
use super::composer::frame_key;
use super::theme::{Palette, font};
use crate::i18n::t;
use crate::state::{ComposerTarget, State, Suggestion, Trigger};

/// The lists for the composers on screen that have something to offer.
pub fn lists(s: &State, p: &Palette) -> Vec<El> {
    [ComposerTarget::Room, ComposerTarget::Thread].into_iter().filter_map(|target| list(s, target, p)).collect()
}

fn list(s: &State, target: ComposerTarget, p: &Palette) -> Option<El> {
    let (query, found) = s.suggestions(target)?;
    let selected = s.composer(target).suggest.selected(&query, found.len());
    let title = match query.trigger {
        Trigger::Person => t!("suggest.people"),
        Trigger::Room => t!("suggest.rooms"),
        Trigger::Emoji => t!("suggest.emoji"),
        Trigger::Command => t!("suggest.commands"),
    };
    let prefix = match target {
        ComposerTarget::Room => "suggestion",
        ComposerTarget::Thread => "thread-suggestion",
    };
    let mut panel = col()
        .key(format!("{prefix}s"))
        .w(400.0)
        .padding(6.0)
        .radius(10.0)
        .fill(p.surface)
        .border(1.0, p.divider)
        .shadow(8.0, 24.0, rune::Color::BLACK.alpha(0.3))
        .preserve_focus_on_press(true)
        .role("listbox")
        .label(title)
        .child(text(title).font(font::SECTION).color(p.text_muted).px(6.0).pt(2.0).pb(4.0));
    for (index, suggestion) in found.into_iter().enumerate() {
        let picked = index == selected;
        let entry = entry(s, &suggestion, p)
            .key(format!("{prefix}-{}", suggestion.id()))
            .w_full()
            .h(36.0)
            .px(10.0)
            .gap(10.0)
            .center_y()
            .radius(6.0)
            .fill(if picked { p.hover.alpha(p.hover.a * 3.0) } else { rune::Color::TRANSPARENT })
            .hover(p.hover.alpha(p.hover.a * 2.0))
            .role("option")
            .checked(picked)
            .cursor(rune::Cursor::Pointer)
            .focusable(false)
            .on_press(move |s: &mut State| s.pick_suggestion(target, &suggestion));
        panel = panel.child(entry);
    }
    Some(col().child(panel).anchored(frame_key(target), Side::Top, Align::Start, 6.0))
}

/// One suggestion's look: a picture or glyph, its name, and a hint.
fn entry(s: &State, suggestion: &Suggestion, p: &Palette) -> El {
    let (leading, name, hint): (El, String, String) = match suggestion {
        Suggestion::Person { user_id, name, avatar } => {
            (media_avatar(user_id, name, avatar.as_ref(), &s.media, 24.0), name.clone(), user_id.clone())
        }
        Suggestion::Everyone => (
            el().size(24.0, 24.0)
                .radius(12.0)
                .fill(p.warning.alpha(0.2))
                .center()
                .child(text("@").font(font::BODY_STRONG).color(p.text)),
            "@room".to_owned(),
            t!("suggest.everyone").to_owned(),
        ),
        Suggestion::Room { room_id, name, avatar } => (
            media_avatar(room_id, name, avatar.as_ref(), &s.media, 24.0).radius(6.0),
            format!("#{name}"),
            String::new(),
        ),
        Suggestion::Emoji { emoji, code } => {
            (text(*emoji).font(rune::TextStyle::new(20.0, 400.0)).w(24.0), format!(":{code}:"), String::new())
        }
        Suggestion::Custom(image) => {
            let picture = el().size(24.0, 24.0).radius(4.0);
            let picture = match s.media.image(&image.media) {
                Some(loaded) => picture.pixels_fit(loaded.pixels(), rune::SurfaceFit::Fit),
                None => picture.fill(p.hover),
            };
            let hint = if image.body == image.shortcode { t!("emoji.custom").to_owned() } else { image.body.clone() };
            (picture, format!(":{}:", image.shortcode), hint)
        }
        Suggestion::Command(command) => (
            el().size(24.0, 24.0).center().child(text("/").font(font::BODY_STRONG).color(p.text_muted)),
            format!("/{} {}", command.name(), command.arguments()).trim_end().to_owned(),
            command.description().to_owned(),
        ),
    };
    row()
        .child(leading)
        .child(text(name).font(font::BODY_STRONG).color(p.text).shrink().truncate())
        .child(text(hint).font(font::META).color(p.text_muted).grow(1.0).min_w(0.0).truncate())
}
