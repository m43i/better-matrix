//! What a composer offers for the word being typed: people after `@` (and `@room`), rooms after
//! `#`, emoji and the packs' custom emoji after `:` and two letters, and slash commands at the
//! start of a message. Picking one puts a pill in the field for a mention or custom emoji, the
//! emoji itself, or the command's name ([`super::composer::Composer::accept_suggestion`]).
//!
//! The list is worked out from the field and what the account knows each time the view asks
//! ([`State::suggestions`]); a composer keeps only which entry is picked and whether the user
//! dismissed the list, for the word they were typing ([`Suggest`]).

use std::ops::Range;

use matrix_core::{MediaRef, PackImage};
use rune::{Block, Mark, TextEdit};

use super::slash::SlashCommand;
use super::{ComposerTarget, State};

/// The most suggestions listed.
pub const MOST: usize = 8;

/// What the word being typed asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Trigger {
    Person,
    Room,
    Emoji,
    Command,
}

/// The word being typed that suggestions are for: its trigger, where it is in the field, and
/// what follows the trigger character.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Query {
    pub trigger: Trigger,
    pub range: Range<usize>,
    pub text: String,
}

/// The word before the caret, if it starts with a trigger: `@` or `#` anywhere, `:` with two
/// letters after it, `/` at the start of the message. Never inside code (a code fence being
/// typed too), a pill, or a selection.
pub fn query(edit: &TextEdit) -> Option<Query> {
    if edit.anchor != edit.focus || edit.active_block() == Block::Code || super::composer::in_code_fence(edit) {
        return None;
    }
    let caret = edit.focus.min(edit.text.len());
    let before = edit.text.get(..caret)?;
    let mut start = before.char_indices().rev().find(|(_, c)| c.is_whitespace()).map_or(0, |(i, c)| i + c.len_utf8());
    for (range, _) in edit.pills() {
        if range.start < caret && caret < range.end {
            return None;
        }
        // A word runs back to a pill's end at most.
        if range.end <= caret && range.end > start {
            start = range.end;
        }
    }
    let in_code = edit.marks().iter().any(|mark| mark.mark == Mark::Code && mark.start <= start && start < mark.end);
    if in_code {
        return None;
    }
    let word = &edit.text[start..caret];
    let mut chars = word.chars();
    let sigil = chars.next()?;
    let text = chars.as_str().to_owned();
    let trigger = match sigil {
        '@' => Trigger::Person,
        '#' => Trigger::Room,
        ':' if text.chars().count() >= 2 && !text.contains(':') => Trigger::Emoji,
        '/' if edit.text[..start].trim().is_empty() && !text.contains('/') => Trigger::Command,
        _ => return None,
    };
    Some(Query { trigger, range: start..caret, text })
}

/// Something a composer can put in for the word being typed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Suggestion {
    /// A room member, mentioned by name.
    Person { user_id: String, name: String, avatar: Option<MediaRef> },
    /// Everyone in the room (`@room`).
    Everyone,
    /// A joined room, mentioned by name.
    Room { room_id: String, name: String, avatar: Option<MediaRef> },
    /// A Unicode emoji, with its short code.
    Emoji { emoji: &'static str, code: String },
    /// A custom emoji from an image pack.
    Custom(PackImage),
    /// A slash command, at the start of a message.
    Command(SlashCommand),
}

impl Suggestion {
    /// Stable within one list: for element keys.
    pub fn id(&self) -> String {
        match self {
            Self::Person { user_id, .. } => format!("person-{user_id}"),
            Self::Everyone => "everyone".to_owned(),
            Self::Room { room_id, .. } => format!("room-{room_id}"),
            Self::Emoji { code, .. } => format!("emoji-{code}"),
            Self::Custom(image) => format!("custom-{}", image.url),
            Self::Command(command) => format!("command-{}", command.name()),
        }
    }
}

/// A composer's hold on its suggestions: which one is picked, for which word, and a word whose
/// list the user dismissed (Escape). Both belong to the word they were set for: typing on keeps
/// them, a new word starts over.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Suggest {
    /// The picked entry and the word it was picked for (where it starts, what it said).
    picked: Option<(usize, String, usize)>,
    /// Where the word whose list was dismissed starts.
    dismissed: Option<usize>,
}

impl Suggest {
    /// The entry picked for `query`: the first unless the user moved for this very word.
    pub fn selected(&self, query: &Query, count: usize) -> usize {
        match &self.picked {
            Some((start, text, index)) if *start == query.range.start && *text == query.text => {
                (*index).min(count.saturating_sub(1))
            }
            _ => 0,
        }
    }

    /// Moves the pick down (or up, for a negative `by`), round past either end.
    pub fn step(&mut self, query: &Query, count: usize, by: isize) {
        if count == 0 {
            return;
        }
        let current = self.selected(query, count) as isize;
        let next = (current + by).rem_euclid(count as isize) as usize;
        self.picked = Some((query.range.start, query.text.clone(), next));
    }

    /// Hides the list for the word being typed.
    pub fn dismiss(&mut self, query: &Query) {
        self.dismissed = Some(query.range.start);
    }

    /// Whether the user dismissed the list for `query`'s word.
    pub fn is_dismissed(&self, query: &Query) -> bool {
        self.dismissed == Some(query.range.start)
    }

    /// Forgets the pick and the dismissal: the field changed under them (sent, a draft back).
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

/// Whether `name` matches what was typed: a word of it starts with it, ignoring case; nothing
/// typed matches everything.
fn matches(name: &str, typed: &str) -> bool {
    typed.is_empty() || super::search::matches(name, typed)
}

/// Unicode emoji whose short code starts with `typed`, then those whose name holds it.
fn emoji_for(typed: &str) -> Vec<Suggestion> {
    let typed = typed.to_lowercase();
    let mut first = Vec::new();
    let mut then = Vec::new();
    for emoji in emojis::iter() {
        if let Some(code) = emoji.shortcodes().find(|code| code.starts_with(typed.as_str())) {
            first.push(Suggestion::Emoji { emoji: emoji.as_str(), code: code.to_owned() });
        } else if emoji.name().to_lowercase().contains(typed.as_str())
            && let Some(code) = emoji.shortcode()
        {
            then.push(Suggestion::Emoji { emoji: emoji.as_str(), code: code.to_owned() });
        }
        if first.len() >= MOST {
            break;
        }
    }
    first.extend(then);
    first
}

impl State {
    /// The word being typed in a composer and what is offered for it, unless the user dismissed
    /// the list or nothing matches. Empty while a memo records or link entry is open.
    pub fn suggestions(&self, target: ComposerTarget) -> Option<(Query, Vec<Suggestion>)> {
        let composer = self.composer(target);
        if composer.voice.is_some() || composer.link.is_some() || !composer.edit.focused {
            return None;
        }
        let query = query(&composer.edit)?;
        if composer.suggest.is_dismissed(&query) {
            return None;
        }
        let account = self.active_account()?;
        let room_id = self.active_room_id()?;
        let typed = query.text.as_str();
        let mut found = Vec::new();
        match query.trigger {
            Trigger::Person => {
                let me = account.summary.id.as_str();
                if let Some(members) = account.members.get(room_id) {
                    found.extend(
                        members
                            .people
                            .iter()
                            .filter(|member| member.person.id != me)
                            .filter(|member| {
                                matches(&member.person.name, typed)
                                    || member.person.id[1..].to_lowercase().starts_with(&typed.to_lowercase())
                            })
                            .take(MOST)
                            .map(|member| Suggestion::Person {
                                user_id: member.person.id.clone(),
                                name: member.person.name.clone(),
                                avatar: member.person.avatar.clone(),
                            }),
                    );
                }
                let room_mention = "room";
                if room_mention.starts_with(&typed.to_lowercase()) && !typed.is_empty() || typed.is_empty() {
                    found.push(Suggestion::Everyone);
                }
            }
            Trigger::Room => found.extend(
                account
                    .rooms
                    .iter()
                    .filter(|room| room.kind == matrix_core::RoomKind::Group && matches(&room.name, typed))
                    .take(MOST)
                    .map(|room| Suggestion::Room {
                        room_id: room.room_id.clone(),
                        name: room.name.clone(),
                        avatar: room.avatar.clone(),
                    }),
            ),
            Trigger::Emoji => {
                let typed_lower = typed.to_lowercase();
                found.extend(
                    self.image_packs(&account.summary.id, room_id)
                        .flat_map(|pack| pack.images.iter())
                        .filter(|image| image.emoticon && image.shortcode.to_lowercase().contains(&typed_lower))
                        .take(MOST)
                        .cloned()
                        .map(Suggestion::Custom),
                );
                found.extend(emoji_for(typed));
            }
            Trigger::Command => {
                // `/meet` only where the homeserver offers meetings.
                let meets = account.room(room_id).is_some_and(|room| room.can_start_meeting);
                found.extend(
                    SlashCommand::ALL
                        .into_iter()
                        .filter(|command| *command != SlashCommand::Meet || meets)
                        .filter(|command| command.name().starts_with(&typed.to_lowercase()))
                        .map(Suggestion::Command),
                );
            }
        }
        found.truncate(MOST);
        (!found.is_empty()).then_some((query, found))
    }

    /// Moves the picked suggestion with a key, takes it, or dismisses the list; true when the
    /// key was for the list (it then doesn't reach the field).
    pub fn suggestion_key(&mut self, target: ComposerTarget, key: &rune::KeyEvent) -> bool {
        use rune::Key;
        let Some((query, found)) = self.suggestions(target) else { return false };
        if key.mods.ctrl || key.mods.alt || key.mods.cmd || key.mods.logo {
            return false;
        }
        let count = found.len();
        match key.key {
            Key::Down => self.composer_mut(target).suggest.step(&query, count, 1),
            Key::Up => self.composer_mut(target).suggest.step(&query, count, -1),
            Key::Escape => self.composer_mut(target).suggest.dismiss(&query),
            Key::Tab | Key::Enter if !key.mods.shift => {
                let index = self.composer(target).suggest.selected(&query, count);
                if let Some(suggestion) = found.into_iter().nth(index) {
                    self.composer_mut(target).accept_suggestion(&query, &suggestion);
                }
            }
            _ => return false,
        }
        true
    }

    /// Takes a suggestion pressed in the list.
    pub fn pick_suggestion(&mut self, target: ComposerTarget, suggestion: &Suggestion) {
        let Some((query, _)) = self.suggestions(target) else { return };
        self.composer_mut(target).accept_suggestion(&query, suggestion);
    }

    /// Asks for what a word being typed needs and isn't here yet: a room's members after `@`,
    /// the packs after `:`. Called from `poll`, so each is asked for once, not per frame.
    pub(super) fn load_for_suggestions(&mut self) {
        for target in [ComposerTarget::Room, ComposerTarget::Thread] {
            let composer = self.composer(target);
            let Some(query) = query(&composer.edit).filter(|_| composer.edit.focused) else { continue };
            let (Some(account), Some(room_id)) = (self.nav.account.clone(), self.active_room_id().map(str::to_owned))
            else {
                continue;
            };
            match query.trigger {
                Trigger::Person => {
                    let listed = self.accounts.get(&account).is_some_and(|data| data.members.contains_key(&room_id));
                    if !listed {
                        self.load_members(account, room_id, false);
                    }
                }
                Trigger::Emoji => {
                    self.want_image_packs(account.clone(), room_id.clone());
                    let wanted = self
                        .custom_emoji(&account, &room_id)
                        .into_iter()
                        .map(|image| (image.media.clone(), super::PACK_IMAGE_SIZE, Some((128, 128))))
                        .collect();
                    self.request_media(&account, wanted);
                }
                Trigger::Room | Trigger::Command => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(text: &str) -> TextEdit {
        let mut edit = TextEdit::new("").rich().allow_marks([Mark::Code, Mark::Pill(String::new())]);
        edit.insert(text);
        edit
    }

    #[test]
    fn an_at_sign_should_ask_for_people() {
        let query = query(&field("hi @gr")).expect("a query");
        assert_eq!((query.trigger, query.range, query.text.as_str()), (Trigger::Person, 3..6, "gr"));
    }

    #[test]
    fn a_lone_at_sign_should_list_everyone() {
        assert_eq!(query(&field("@")).map(|query| query.text), Some(String::new()));
    }

    #[test]
    fn nothing_should_be_suggested_inside_a_code_fence() {
        assert_eq!(query(&field("```\n@ro")), None);
        assert!(query(&field("```\ncode\n```\n@ro")).is_some(), "after the fence closes");
    }

    #[test]
    fn a_hash_should_ask_for_rooms() {
        assert_eq!(query(&field("see #des")).map(|query| query.trigger), Some(Trigger::Room));
    }

    #[test]
    fn a_colon_should_wait_for_two_letters() {
        assert_eq!(query(&field("nice :t")), None);
        assert_eq!(query(&field("nice :th")).map(|query| query.trigger), Some(Trigger::Emoji));
    }

    #[test]
    fn a_finished_short_code_should_ask_for_nothing() {
        assert_eq!(query(&field(":smile:")), None);
    }

    #[test]
    fn a_slash_should_ask_for_commands_only_at_the_start() {
        assert_eq!(query(&field("/sh")).map(|query| query.trigger), Some(Trigger::Command));
        assert_eq!(query(&field("and/or")), None);
        assert_eq!(query(&field("this /sh")), None);
    }

    #[test]
    fn a_word_after_a_space_should_ask_for_nothing() {
        assert_eq!(query(&field("@grace ")), None);
    }

    #[test]
    fn a_word_should_start_after_a_pill() {
        let mut edit = field("");
        edit.insert_pill("user:@a:x", "Ada");
        edit.insert("@g");
        let query = query(&edit).expect("a query");
        assert_eq!(query.range, 3..5);
    }

    #[test]
    fn code_should_ask_for_nothing() {
        let mut edit = field("");
        edit.toggle_mark(Mark::Code);
        edit.insert("@x");
        assert_eq!(query(&edit), None);
    }

    #[test]
    fn the_pick_should_go_round_past_either_end() {
        let query = query(&field("@a")).expect("a query");
        let mut suggest = Suggest::default();
        suggest.step(&query, 3, -1);
        assert_eq!(suggest.selected(&query, 3), 2);
        suggest.step(&query, 3, 1);
        assert_eq!(suggest.selected(&query, 3), 0);
    }

    #[test]
    fn typing_on_should_pick_the_first_again() {
        let mut suggest = Suggest::default();
        suggest.step(&query(&field("@a")).expect("a query"), 3, 1);
        assert_eq!(suggest.selected(&query(&field("@ad")).expect("a query"), 3), 0);
    }

    #[test]
    fn a_dismissed_list_should_stay_away_while_its_word_is_typed() {
        let mut suggest = Suggest::default();
        suggest.dismiss(&query(&field("@a")).expect("a query"));
        assert!(suggest.is_dismissed(&query(&field("@ad")).expect("a query")));
        assert!(!suggest.is_dismissed(&query(&field("@ad @b")).expect("a query")));
    }

    #[test]
    fn emoji_should_be_found_by_their_short_code_first() {
        let found = emoji_for("thumbs");
        assert!(matches!(found.first(), Some(Suggestion::Emoji { emoji: "👍", .. })), "{found:?}");
    }
}
