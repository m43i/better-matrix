//! The emoji picker: a search over every emoji by name and shortcode, else quick reactions and
//! emoji by group. It reacts to a message or types into a composer, by what opened it.

use rune::{Align, El, ScrollSpec, Side, col, el, icon, row, text, text_input};

use super::components::{divider, popover_panel};
use super::theme::{Palette, font};
use crate::i18n::t;
use crate::state::{EmojiFor, State};

/// How many search results show at most.
const RESULTS: usize = 96;

/// The emoji whose name or a shortcode holds every word of `query`.
pub fn search(query: &str) -> Vec<&'static str> {
    let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    if words.is_empty() {
        return Vec::new();
    }
    emojis::iter()
        .filter(|emoji| {
            let name = emoji.name().to_lowercase();
            words
                .iter()
                .all(|word| name.contains(word.as_str()) || emoji.shortcodes().any(|code| code.contains(word.as_str())))
        })
        .map(emojis::Emoji::as_str)
        .take(RESULTS)
        .collect()
}

/// The reactions most messages get, first in the picker.
pub const QUICK: [&str; 8] = ["👍", "❤️", "😂", "🎉", "😮", "😢", "🙏", "👀"];

/// Emoji by group, as the picker lists them.
fn groups() -> [(&'static str, &'static [&'static str]); 6] {
    [
        (
            t!("emoji.smileys"),
            &[
                "😀", "😃", "😄", "😁", "😆", "😅", "🤣", "😂", "🙂", "🙃", "😉", "😊", "😇", "🥰", "😍", "🤩", "😘",
                "😋", "😛", "😜", "🤪", "🤔", "🤨", "😐", "😑", "😶", "🙄", "😏", "😬", "😌", "😴", "🤯", "🥳", "😎",
                "🤓", "😕", "😟", "🙁", "😮", "😲", "😳", "🥺", "😢", "😭", "😱", "😤", "😡", "🤬", "💀", "🤡",
            ],
        ),
        (
            t!("emoji.people"),
            &[
                "👍", "👎", "👌", "✌️", "🤞", "🤟", "🤘", "👋", "🤙", "💪", "🙏", "👏", "🙌", "🤝", "👀", "🧠", "🫡",
                "🤷", "🤦", "🙋", "🙇", "💁",
            ],
        ),
        (
            t!("emoji.nature"),
            &[
                "🐶", "🐱", "🦊", "🐻", "🐼", "🐨", "🦁", "🐸", "🐙", "🦄", "🐝", "🌱", "🌻", "🌈", "⭐", "🔥", "⚡",
                "❄️", "🌊",
            ],
        ),
        (t!("emoji.food"), &["☕", "🍵", "🍺", "🍷", "🥂", "🍕", "🍔", "🌮", "🍣", "🍩", "🍪", "🎂", "🍎", "🍌", "🥑"]),
        (
            t!("emoji.activities"),
            &[
                "🎉", "🎊", "🎈", "🏆", "🥇", "⚽", "🎮", "🎯", "🎸", "🚀", "✈️", "🏠", "💻", "📱", "📌", "📎", "🔒",
                "💡",
            ],
        ),
        (
            t!("emoji.symbols"),
            &[
                "❤️", "🧡", "💛", "💚", "💙", "💜", "🖤", "💯", "✅", "❌", "⚠️", "❓", "❗", "➕", "➖", "🔁", "🆗",
                "🆕",
            ],
        ),
    ]
}

/// The picker, anchored to the button that opened it: a message's react button, or a
/// composer's emoji button. A press on it leaves the composer's field focused.
pub fn picker(s: &State, chosen_for: &EmojiFor, anchor: String, p: &Palette) -> El {
    // Quick reactions repeat emoji from the groups: their keys say which they are.
    let button = |emoji: &'static str, size: f32, place: &str| {
        let chosen_for = chosen_for.clone();
        el().key(format!("emoji-{place}-{emoji}"))
            .size(size, size)
            .radius(6.0)
            .center()
            .hover(p.hover.alpha(p.hover.a * 3.0))
            .role("button")
            .cursor(rune::Cursor::Pointer)
            .label(emoji)
            .child(text(emoji).font(rune::TextStyle::new(size * 0.6, 400.0)))
            .on_press(move |s: &mut State| s.choose_emoji(chosen_for.clone(), emoji))
    };
    let query = s.emoji_search.text.trim();
    let found = search(query);
    let first = found.first().copied();
    let submit_for = chosen_for.clone();
    let field = text_input(&s.emoji_search, font::BODY)
        .key("emoji-search")
        .single_line()
        .placeholder(t!("emoji.search"), p.text_muted)
        .color(p.text)
        .grow(1.0)
        .on_edit(|s: &mut State| &mut s.emoji_search)
        .on_input_key(State::popover_input_key)
        // Enter picks the first match.
        .on_submit(move |s: &mut State, _| {
            if let Some(emoji) = first {
                s.choose_emoji(submit_for.clone(), emoji);
            }
        });
    let search_bar = row()
        .grow(1.0)
        .min_w(0.0)
        .h(34.0)
        .px(10.0)
        .gap(8.0)
        .center_y()
        .radius(8.0)
        .fill(p.bg)
        .border(1.0, p.divider)
        .child(icon(&super::icons::SEARCH, 15.0).color(p.text_muted))
        .child(field);
    let mut panel = popover_panel(p)
        .key("emoji-picker")
        .preserve_focus_on_press(true)
        .w(344.0)
        .child(row().w_full().px(8.0).pt(8.0).pb(4.0).child(search_bar));
    let mut grid = col().w_full().px(8.0).pb(8.0).gap(6.0);
    // A composer's picker leads with the room's custom emoji.
    if let EmojiFor::Composer(target) = chosen_for
        && let (Some(account), Some(room_id)) = (s.nav.account.as_ref(), s.active_room_id())
    {
        let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
        let custom: Vec<_> = s
            .custom_emoji(account, room_id)
            .into_iter()
            .filter(|image| words.iter().all(|word| image.shortcode.to_lowercase().contains(word.as_str())))
            .collect();
        if !custom.is_empty() {
            let target = *target;
            let set = custom.into_iter().fold(row().w_full().flex_wrap(), |set, image| {
                let picture = el().size(24.0, 24.0);
                let picture = match s.media.image(&image.media) {
                    Some(loaded) => picture.pixels_fit(loaded.pixels(), rune::SurfaceFit::Fit),
                    None => picture.radius(4.0).fill(p.hover),
                };
                let chosen = image.clone();
                set.child(
                    el().key(format!("emoji-custom-{}", image.url))
                        .size(32.0, 32.0)
                        .radius(6.0)
                        .center()
                        .hover(p.hover.alpha(p.hover.a * 3.0))
                        .role("button")
                        .cursor(rune::Cursor::Pointer)
                        .label(format!(":{}:", image.shortcode))
                        .help(format!(":{}:", image.shortcode))
                        .child(picture)
                        .on_press(move |s: &mut State| s.choose_custom_emoji(target, &chosen)),
                )
            });
            grid =
                grid.child(text(t!("emoji.custom")).font(font::SECTION).color(p.text_muted).px(4.0).pt(4.0)).child(set);
        }
    }
    if query.is_empty() {
        let quick =
            QUICK.iter().fold(row().w_full().px(8.0).gap(2.0), |row, emoji| row.child(button(emoji, 36.0, "quick")));
        panel = panel
            .child(text(t!("emoji.quick")).font(font::SECTION).color(p.text_muted).px(12.0).py(6.0))
            .child(quick)
            .child(divider(p));
        for (title, emojis) in groups() {
            let set =
                emojis.iter().fold(row().w_full().flex_wrap(), |set, emoji| set.child(button(emoji, 32.0, "set")));
            grid = grid.child(text(title).font(font::SECTION).color(p.text_muted).px(4.0).pt(4.0)).child(set);
        }
    } else if found.is_empty() {
        grid = grid.child(text(t!("emoji.none")).font(font::BODY).color(p.text_muted).px(4.0).py(8.0));
    } else {
        grid = grid.child(
            found.iter().fold(row().w_full().pt(4.0).flex_wrap(), |set, emoji| set.child(button(emoji, 32.0, "found"))),
        );
    }
    let panel = panel
        .child(col().key("emoji-grid").w_full().max_h(300.0).scroll(ScrollSpec { stick_bottom: false }).child(grid));
    match chosen_for {
        EmojiFor::StatusMessage => col().child(panel).anchored(anchor, Side::Right, Align::End, 10.0),
        _ => col().child(panel).anchored(anchor, Side::Top, Align::End, 6.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn searching_by_name_should_find_the_emoji() {
        assert!(search("thumbs up").contains(&"👍"));
    }

    #[test]
    fn searching_by_shortcode_should_find_the_emoji() {
        assert!(search("tada").contains(&"🎉"));
    }

    #[test]
    fn an_empty_search_should_find_nothing() {
        assert!(search("  ").is_empty());
    }
}
