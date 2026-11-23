//! The emoji picker: a search over every emoji by name and shortcode, else quick reactions and
//! every emoji by group, with a tab per group to jump to it. It reacts to a message or types into a composer, by what opened it.

use std::borrow::Cow;

use matrix_core::PackImage;
use rune::{Align, El, ScrollSpec, Side, col, el, icon, row, text, text_input};

use super::components::{divider, popover_panel};
use super::theme::{Palette, font};
use crate::i18n::t;
use crate::state::{ComposerTarget, EmojiFor, EmojiStyle, State};

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

/// How many emoji a row of the picker's grid holds: its width over a cell's.
const COLUMNS: usize = 10;

/// A cell of the grid, and the height of one of its rows.
const CELL: f32 = 32.0;

/// One of the picker's groups: a stable name for its keys, the emoji its tab shows, and every
/// emoji in it, skin tones left out.
struct Section {
    group: emojis::Group,
    id: &'static str,
    tab: &'static str,
    emoji: Vec<&'static str>,
}

impl Section {
    /// The key of its title row, which its tab scrolls to.
    fn key(&self) -> String {
        format!("emoji-group-{}", self.id)
    }
}

/// The groups in Unicode's order, as the picker lists them before a search. Built once.
fn sections() -> &'static [Section] {
    use emojis::Group;
    const GROUPS: [(Group, &str, &str); 9] = [
        (Group::SmileysAndEmotion, "smileys", "😀"),
        (Group::PeopleAndBody, "people", "👋"),
        (Group::AnimalsAndNature, "nature", "🐶"),
        (Group::FoodAndDrink, "food", "🍔"),
        (Group::TravelAndPlaces, "travel", "🚗"),
        (Group::Activities, "activities", "⚽"),
        (Group::Objects, "objects", "💡"),
        (Group::Symbols, "symbols", "❤️"),
        (Group::Flags, "flags", "🏁"),
    ];
    static SECTIONS: std::sync::LazyLock<Vec<Section>> = std::sync::LazyLock::new(|| {
        GROUPS
            .into_iter()
            .map(|(group, id, tab)| Section {
                group,
                id,
                tab,
                emoji: group.emojis().map(emojis::Emoji::as_str).collect(),
            })
            .collect()
    });
    &SECTIONS
}

/// A group's title in the picker.
fn group_title(group: emojis::Group) -> &'static str {
    use emojis::Group;
    match group {
        Group::SmileysAndEmotion => t!("emoji.smileys"),
        Group::PeopleAndBody => t!("emoji.people"),
        Group::AnimalsAndNature => t!("emoji.nature"),
        Group::FoodAndDrink => t!("emoji.food"),
        Group::TravelAndPlaces => t!("emoji.travel"),
        Group::Activities => t!("emoji.activities"),
        Group::Objects => t!("emoji.objects"),
        Group::Symbols => t!("emoji.symbols"),
        Group::Flags => t!("emoji.flags"),
    }
}

/// One row of the picker's grid before a search: a title, or up to [`COLUMNS`] emoji. A row's
/// key names what it holds, never where it sits, so custom emoji arriving above it keep the
/// place in the grid.
enum Row {
    /// The room's custom emoji's title.
    CustomTitle,
    /// A row of the room's custom emoji.
    Custom(Vec<PackImage>),
    Title(&'static Section),
    Emoji(&'static [&'static str]),
}

/// Every group's rows with their keys, in order. Built once.
fn unicode_rows() -> &'static [(String, Row)] {
    static ROWS: std::sync::LazyLock<Vec<(String, Row)>> = std::sync::LazyLock::new(|| {
        sections()
            .iter()
            .flat_map(|section| {
                let emoji = section.emoji.chunks(COLUMNS).map(|chunk| {
                    (format!("emoji-row-{}", chunk.first().copied().unwrap_or_default()), Row::Emoji(chunk))
                });
                std::iter::once((section.key(), Row::Title(section))).chain(emoji)
            })
            .collect()
    });
    &ROWS
}

/// Row `i` of the grid when it is one of the groups' rows, after `custom` rows of custom emoji.
fn unicode_row(i: usize, custom: usize) -> Option<&'static (String, Row)> {
    i.checked_sub(custom).and_then(|i| unicode_rows().get(i))
}

/// What tells a custom emoji apart: its image and short code, as a pack may give one image
/// several short codes.
fn custom_id(image: &PackImage) -> String {
    format!("{}:{}", image.url, image.shortcode)
}

/// The custom emoji whose short code holds every word of `query`, each once though several
/// packs hold it.
fn custom_matches(images: Vec<&PackImage>, query: &str) -> Vec<PackImage> {
    let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    let mut seen = std::collections::HashSet::new();
    images
        .into_iter()
        .filter(|image| words.iter().all(|word| image.shortcode.to_lowercase().contains(word.as_str())))
        .filter(|image| seen.insert(custom_id(image)))
        .cloned()
        .collect()
}

/// The rows of a room's custom emoji, with their keys: none without any.
fn custom_rows(custom: Vec<PackImage>) -> Vec<(String, Row)> {
    if custom.is_empty() {
        return Vec::new();
    }
    let rows = custom.chunks(COLUMNS).map(|chunk| {
        (format!("emoji-custom-row-{}", chunk.first().map(custom_id).unwrap_or_default()), Row::Custom(chunk.to_vec()))
    });
    std::iter::once(("emoji-custom-title".to_owned(), Row::CustomTitle)).chain(rows).collect()
}

/// A Unicode emoji's cell. `place` tells apart cells showing the same emoji (the quick
/// reactions repeat some of the groups').
fn emoji_button(
    emoji: &'static str,
    size: f32,
    place: &str,
    chosen_for: &EmojiFor,
    style: EmojiStyle,
    p: &Palette,
) -> El {
    let chosen_for = chosen_for.clone();
    el().key(format!("emoji-{place}-{emoji}"))
        .size(size, size)
        .radius(6.0)
        .center()
        .hover(p.hover.alpha(p.hover.a * 3.0))
        .role("button")
        .cursor(rune::Cursor::Pointer)
        .label(emoji)
        .child(super::emoji_art::glyph(emoji, style, p.text, size * 0.6))
        .on_press(move |s: &mut State| s.choose_emoji(chosen_for.clone(), emoji))
}

/// A custom emoji's cell: its picture, typed into the composer `target` as a pill.
fn custom_button(s: &State, image: &PackImage, target: ComposerTarget, p: &Palette) -> El {
    let picture = el().size(24.0, 24.0);
    let picture = match s.media.image(&image.media) {
        Some(loaded) => picture.pixels_fit(loaded.pixels(), rune::SurfaceFit::Fit),
        None => picture.radius(4.0).fill(p.hover),
    };
    let chosen = image.clone();
    el().key(format!("emoji-custom-{}", custom_id(image)))
        .size(CELL, CELL)
        .radius(6.0)
        .center()
        .hover(p.hover.alpha(p.hover.a * 3.0))
        .role("button")
        .cursor(rune::Cursor::Pointer)
        .label(format!(":{}:", image.shortcode))
        .help(format!(":{}:", image.shortcode))
        .child(picture)
        .on_press(move |s: &mut State| s.choose_custom_emoji(target, &chosen))
}

/// A section's title in the grid.
fn title(label: &str, p: &Palette) -> El {
    text(label).font(font::SECTION).color(p.text_muted).px(4.0).pt(8.0).pb(4.0)
}

/// One row of the grid before a search.
fn grid_row(s: &State, line: &Row, chosen_for: &EmojiFor, p: &Palette) -> El {
    match line {
        Row::CustomTitle => title(t!("emoji.custom"), p),
        Row::Title(section) => title(group_title(section.group), p),
        Row::Emoji(emoji) => emoji.iter().fold(row().w_full(), |set, emoji| {
            set.child(emoji_button(emoji, CELL, "set", chosen_for, s.settings.emoji_style, p))
        }),
        Row::Custom(images) => {
            let EmojiFor::Composer(target) = chosen_for else { return el() };
            images.iter().fold(row().w_full(), |set, image| set.child(custom_button(s, image, *target, p)))
        }
    }
}

/// A tab per group, each scrolling the grid to that group's title.
fn group_tabs(p: &Palette) -> El {
    sections().iter().fold(row().w_full().px(8.0).pb(4.0).gap(2.0), |tabs, section| {
        tabs.child(
            el().key(format!("emoji-tab-{}", section.id))
                .size(30.0, 28.0)
                .radius(6.0)
                .center()
                .hover(p.hover.alpha(p.hover.a * 3.0))
                .role("button")
                .cursor(rune::Cursor::Pointer)
                .label(group_title(section.group))
                .help(group_title(section.group))
                .child(text(section.tab).font(rune::TextStyle::new(16.0, 400.0)))
                // At the grid's top, so the group's emoji show below its title.
                .on_press_tx(move |_: &mut State, tx| tx.scroll_key_to(section.key(), 0.0)),
        )
    })
}

/// The picker, anchored to the button that opened it: a message's react button, or a
/// composer's emoji button. A press on it leaves the composer's field focused.
pub fn picker(s: &State, chosen_for: &EmojiFor, anchor: String, p: &Palette) -> El {
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
    // A composer's picker leads with the room's custom emoji.
    let custom: Vec<PackImage> = match chosen_for {
        EmojiFor::Composer(_) => match (s.nav.account.as_ref(), s.active_room_id()) {
            (Some(account), Some(room_id)) => custom_matches(s.custom_emoji(account, room_id), query),
            _ => Vec::new(),
        },
        _ => Vec::new(),
    };
    let grid = if query.is_empty() {
        let quick = QUICK.iter().fold(row().w_full().px(8.0).gap(2.0), |row, emoji| {
            row.child(emoji_button(emoji, 36.0, "quick", chosen_for, s.settings.emoji_style, p))
        });
        panel = panel
            .child(text(t!("emoji.quick")).font(font::SECTION).color(p.text_muted).px(12.0).py(6.0))
            .child(quick)
            .child(divider(p))
            .child(group_tabs(p));
        // Only the rows in view are built: there are thousands of emoji. The room's custom
        // emoji come first, then the groups' rows.
        // The grid opens anew once the room's custom emoji arrive, so they show at its top
        // rather than above the rows in view, which Rune would otherwise hold in place.
        let grid_key = if custom.is_empty() { "emoji-grid" } else { "emoji-grid-custom" };
        let custom = std::rc::Rc::new(custom_rows(custom));
        let version: Vec<String> = custom.iter().map(|(key, _)| key.clone()).collect();
        let keys = custom.clone();
        let (chosen_for, palette) = (chosen_for.clone(), *p);
        let grid = col()
            .key("emoji-rows")
            .w_full()
            .px(8.0)
            .pb(8.0)
            .growth(rune::GrowthSpec::default())
            .virtual_rows(
                custom.len() + unicode_rows().len(),
                CELL,
                move |_: &State, i| match keys.get(i) {
                    Some((key, _)) => Cow::Owned(key.clone()),
                    None => Cow::Borrowed(unicode_row(i, keys.len()).map_or("", |(key, _)| key.as_str())),
                },
                move |s: &State, i| {
                    custom
                        .get(i)
                        .or_else(|| unicode_row(i, custom.len()))
                        .map_or_else(el, |(_, line)| grid_row(s, line, &chosen_for, &palette))
                },
            )
            .virtual_rows_version(version);
        // A fixed height: the rows' own would be an estimate.
        col().key(grid_key).w_full().h(300.0).child(grid)
    } else {
        let mut grid = col().w_full().px(8.0).pb(8.0).gap(6.0);
        if let EmojiFor::Composer(target) = chosen_for
            && !custom.is_empty()
        {
            let set = custom
                .iter()
                .fold(row().w_full().flex_wrap(), |set, image| set.child(custom_button(s, image, *target, p)));
            grid = grid.child(title(t!("emoji.custom"), p)).child(set);
        }
        let grid = if found.is_empty() {
            grid.child(text(t!("emoji.none")).font(font::BODY).color(p.text_muted).px(4.0).py(8.0))
        } else {
            grid.child(found.iter().fold(row().w_full().pt(4.0).flex_wrap(), |set, emoji| {
                set.child(emoji_button(emoji, CELL, "found", chosen_for, s.settings.emoji_style, p))
            }))
        };
        // Only as tall as the results, up to the browsing grid's height.
        col().key("emoji-grid").w_full().max_h(300.0).child(grid)
    };
    let panel = panel.child(grid.scroll(ScrollSpec { stick_bottom: false }));
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

    #[test]
    fn the_groups_should_hold_every_emoji() {
        let listed: usize = sections().iter().map(|section| section.emoji.len()).sum();
        assert_eq!(listed, emojis::iter().count());
    }

    #[test]
    fn the_groups_should_hold_flags() {
        let flags = sections().iter().find(|section| section.group == emojis::Group::Flags);
        assert!(flags.is_some_and(|flags| flags.emoji.contains(&"🇩🇪")));
    }

    #[test]
    fn no_row_should_hold_more_emoji_than_fit_across() {
        assert!(unicode_rows().iter().all(|(_, row)| !matches!(row, Row::Emoji(emoji) if emoji.len() > COLUMNS)));
    }

    fn image(shortcode: &str) -> PackImage {
        PackImage {
            shortcode: shortcode.into(),
            body: shortcode.into(),
            url: "mxc://x/blob".into(),
            media: matrix_core::MediaRef::opaque("mxc://x/blob"),
            emoticon: true,
            sticker: false,
            dimensions: None,
            mimetype: None,
        }
    }

    #[test]
    fn one_image_under_two_short_codes_should_get_two_rows_keys() {
        let custom = std::iter::repeat_with(|| image("blob")).take(COLUMNS).chain([image("blobcat")]).collect();
        let keys: std::collections::HashSet<_> = custom_rows(custom).into_iter().map(|(key, _)| key).collect();
        assert_eq!(keys.len(), 3, "{keys:?}");
    }

    #[test]
    fn a_custom_emoji_in_two_packs_should_be_offered_once() {
        let (blob, again, cat) = (image("blob"), image("blob"), image("blobcat"));
        let found: Vec<_> = custom_matches(vec![&blob, &again, &cat], "").into_iter().map(|i| i.shortcode).collect();
        assert_eq!(found, ["blob", "blobcat"]);
    }

    #[test]
    fn every_row_should_have_its_own_key() {
        let keys: std::collections::HashSet<_> = unicode_rows().iter().map(|(key, _)| key).collect();
        assert_eq!(keys.len(), unicode_rows().len());
    }
}
