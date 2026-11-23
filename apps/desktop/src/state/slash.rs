//! Slash commands: a message starting with `/name` does something instead of, or besides,
//! sending its text, as in other Matrix clients. A command turns into what already exists: an
//! emote or text through the composer's usual send, an invite, a join through the link dialog,
//! leaving through its confirmation, a topic, a display name. `//text` sends `/text`; a name the
//! app doesn't know stops the send with a notice, so a typo never goes out as a message.

use super::{ComposerTarget, Purpose, State};
use crate::i18n::{t, tf};
use matrix_core::{Command, Outgoing};

/// Every slash command, in the order suggestions list them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlashCommand {
    Me,
    Shrug,
    TableFlip,
    Unflip,
    Lenny,
    Plain,
    Spoiler,
    Invite,
    Join,
    Leave,
    Topic,
    Nick,
}

impl SlashCommand {
    pub const ALL: [SlashCommand; 12] = [
        Self::Me,
        Self::Shrug,
        Self::TableFlip,
        Self::Unflip,
        Self::Lenny,
        Self::Plain,
        Self::Spoiler,
        Self::Invite,
        Self::Join,
        Self::Leave,
        Self::Topic,
        Self::Nick,
    ];

    /// What is typed after the slash; the same in every language.
    pub fn name(self) -> &'static str {
        match self {
            Self::Me => "me",
            Self::Shrug => "shrug",
            Self::TableFlip => "tableflip",
            Self::Unflip => "unflip",
            Self::Lenny => "lenny",
            Self::Plain => "plain",
            Self::Spoiler => "spoiler",
            Self::Invite => "invite",
            Self::Join => "join",
            Self::Leave => "leave",
            Self::Topic => "topic",
            Self::Nick => "nick",
        }
    }

    /// What it does, for its suggestion.
    pub fn description(self) -> &'static str {
        match self {
            Self::Me => t!("slash.me"),
            Self::Shrug => t!("slash.shrug"),
            Self::TableFlip => t!("slash.tableflip"),
            Self::Unflip => t!("slash.unflip"),
            Self::Lenny => t!("slash.lenny"),
            Self::Plain => t!("slash.plain"),
            Self::Spoiler => t!("slash.spoiler"),
            Self::Invite => t!("slash.invite"),
            Self::Join => t!("slash.join"),
            Self::Leave => t!("slash.leave"),
            Self::Topic => t!("slash.topic"),
            Self::Nick => t!("slash.nick"),
        }
    }

    /// What it takes after its name, for its suggestion; empty when nothing.
    pub fn arguments(self) -> &'static str {
        match self {
            Self::Me | Self::Plain | Self::Spoiler => t!("slash.args.message"),
            Self::Shrug | Self::TableFlip | Self::Unflip | Self::Lenny => t!("slash.args.optional_message"),
            Self::Invite => t!("slash.args.user"),
            Self::Join => t!("slash.args.room"),
            Self::Topic => t!("slash.args.topic"),
            Self::Nick => t!("slash.args.name"),
            Self::Leave => "",
        }
    }

    /// Whether it makes a message (and so can be scheduled), rather than doing something.
    pub fn sends_message(self) -> bool {
        matches!(
            self,
            Self::Me | Self::Plain | Self::Spoiler | Self::Shrug | Self::TableFlip | Self::Unflip | Self::Lenny
        )
    }

    /// The text a face command adds to the message.
    fn face(self) -> Option<&'static str> {
        match self {
            Self::Shrug => Some("¯\\_(ツ)_/¯"),
            Self::TableFlip => Some("(╯°□°)╯︵ ┻━┻"),
            Self::Unflip => Some("┬──┬ ノ( ゜-゜ノ)"),
            Self::Lenny => Some("( ͡° ͜ʖ ͡°)"),
            _ => None,
        }
    }
}

/// What a message's text is, read as a possible command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Parsed {
    /// Text to send as it is.
    Message,
    /// `//text`: text that starts with a slash; the first goes.
    Escaped,
    /// A command, its name taking the text's first `len` bytes (leading space included).
    Command { command: SlashCommand, len: usize },
    /// `/name` the app doesn't know.
    Unknown(String),
}

/// Reads a message's text as a command: `/name`, then the end or a space. A slash followed by
/// anything else (`/usr/bin`) is a message.
pub fn parse(text: &str) -> Parsed {
    let trimmed = text.trim_start();
    let lead = text.len() - trimmed.len();
    if trimmed.starts_with("//") {
        return Parsed::Escaped;
    }
    let Some(rest) = trimmed.strip_prefix('/') else { return Parsed::Message };
    let name_len = rest.find(|c: char| !c.is_ascii_alphanumeric()).unwrap_or(rest.len());
    let (name, after) = rest.split_at(name_len);
    if name.is_empty() || !(after.is_empty() || after.starts_with(char::is_whitespace)) {
        return Parsed::Message;
    }
    let name = name.to_ascii_lowercase();
    match SlashCommand::ALL.into_iter().find(|command| command.name() == name) {
        Some(command) => Parsed::Command { command, len: lead + 1 + name_len },
        None => Parsed::Unknown(name),
    }
}

impl State {
    /// Runs a composer's text as a command when it is one. Returns the messages to send in its
    /// place: `None` to send the composer as usual, `Some(empty)` when the command did all
    /// there was to do (or stopped with a notice).
    pub(super) fn run_command(&mut self, target: ComposerTarget) -> Option<Vec<Outgoing>> {
        // A slash in a pill is a spoiler's or a formula's words, sent as written.
        if self.composer(target).starts_in_pill() {
            return None;
        }
        let text = self.composer(target).edit.text.clone();
        let (command, len) = match parse(&text) {
            Parsed::Message => return None,
            Parsed::Escaped => {
                let at = text.len() - text.trim_start().len();
                self.composer_mut(target).cut_front(at + 1);
                return None;
            }
            Parsed::Unknown(name) => {
                self.notice = Some(tf!("slash.unknown", name = name));
                return Some(Vec::new());
            }
            Parsed::Command { command, len } => (command, len),
        };
        let arguments = text[len..].trim().to_owned();
        let (Some(account), Some(key)) = (self.nav.account.clone(), self.composer_key(target)) else {
            return Some(Vec::new());
        };
        let room_id = key.room_id;
        let needs_arguments = !command.arguments().is_empty()
            && !matches!(
                command,
                SlashCommand::Shrug | SlashCommand::TableFlip | SlashCommand::Unflip | SlashCommand::Lenny
            );
        // A command that acts rather than sends reads its arguments as plain words: a spoiler
        // there would show (a topic, a name) to everyone.
        if !command.sends_message() && self.composer(target).spoiler_after(len) {
            self.notice = Some(tf!("slash.no_spoiler", command = command.name()));
            return Some(Vec::new());
        }
        if needs_arguments && arguments.is_empty() {
            self.notice = Some(tf!("slash.usage", command = command.name(), arguments = command.arguments()));
            return Some(Vec::new());
        }
        match command {
            SlashCommand::Spoiler => {
                // Written out here rather than through the field, whose code blocks keep no pills:
                // the words go hidden whatever the field holds them as.
                let composer = self.composer_mut(target);
                composer.cut_front(len);
                let words = composer.edit.text.trim().to_owned();
                // Only the files are kept: the words go as typed, in the spoiler.
                let (reply, messages) = composer.take_outgoing(super::Emoticons::AsTyped);
                composer.reply_to = reply;
                let html = matrix_core::html::spoiler(&words);
                let body = super::composer::SPOILER_FALLBACK.to_owned();
                let spoiler = Outgoing::Text { body, html: Some(html), mentions: Default::default() };
                // The words go hidden in the text's place; the files go as they would.
                let files = messages.into_iter().filter(|message| !matches!(message, Outgoing::Text { .. }));
                Some(std::iter::once(spoiler).chain(files).collect())
            }
            SlashCommand::Me | SlashCommand::Plain => {
                let emoticons = self.settings.emoticons;
                let composer = self.composer_mut(target);
                composer.cut_front(len);
                // `/plain` sends the text as typed, code fences and all.
                let typed = composer.typed_body(emoticons);
                let (reply, messages) = composer.take_outgoing(emoticons);
                let mut messages = messages.into_iter();
                let first = messages.next().map(|message| match message {
                    Outgoing::Text { body, html, mentions } if command == SlashCommand::Me => {
                        Outgoing::Emote { body, html, mentions }
                    }
                    Outgoing::Text { mentions, .. } if command == SlashCommand::Plain => {
                        Outgoing::Text { body: typed.clone(), html: None, mentions }
                    }
                    other => other,
                });
                let messages: Vec<Outgoing> = first.into_iter().chain(messages).collect();
                // The reply goes with them, as with any message.
                self.composer_mut(target).reply_to = reply;
                Some(messages)
            }
            SlashCommand::Shrug | SlashCommand::TableFlip | SlashCommand::Unflip | SlashCommand::Lenny => {
                let composer = self.composer_mut(target);
                composer.cut_front(len);
                if let Some(face) = command.face() {
                    composer.append(face);
                }
                None
            }
            SlashCommand::Invite => {
                let user_id = self
                    .composer(target)
                    .edit
                    .pills()
                    .find_map(|(_, id)| id.strip_prefix("user:").map(str::to_owned))
                    .or_else(|| {
                        arguments.split_whitespace().next().filter(|id| id.starts_with('@')).map(str::to_owned)
                    });
                match user_id.filter(|id| id.contains(':')) {
                    Some(user_id) => {
                        self.dispatch(Command::Invite { account, room_id, user_id }, Purpose::Invite);
                        self.composer_mut(target).clear();
                    }
                    None => {
                        self.notice =
                            Some(tf!("slash.usage", command = command.name(), arguments = command.arguments()));
                    }
                }
                Some(Vec::new())
            }
            SlashCommand::Join => {
                let link = self
                    .composer(target)
                    .edit
                    .pills()
                    .find_map(|(_, id)| id.strip_prefix("room:").map(str::to_owned))
                    .unwrap_or(arguments);
                if matrix_core::Permalink::parse_typed(&link).is_some() {
                    self.composer_mut(target).clear();
                    self.open_typed_link(&link);
                } else {
                    self.notice = Some(tf!("slash.usage", command = command.name(), arguments = command.arguments()));
                }
                Some(Vec::new())
            }
            SlashCommand::Leave => {
                self.composer_mut(target).clear();
                self.ask_to_leave(&room_id);
                Some(Vec::new())
            }
            SlashCommand::Topic => {
                let setting = matrix_core::RoomSetting::Topic(arguments);
                self.dispatch(Command::ChangeRoom { account, room_id, setting }, Purpose::Command);
                self.composer_mut(target).clear();
                Some(Vec::new())
            }
            SlashCommand::Nick => {
                self.dispatch(Command::SetDisplayName { account, name: arguments }, Purpose::Command);
                self.composer_mut(target).clear();
                Some(Vec::new())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_known_name_should_read_as_its_command() {
        assert_eq!(parse("/me waves"), Parsed::Command { command: SlashCommand::Me, len: 3 });
    }

    #[test]
    fn a_command_should_read_without_arguments() {
        assert_eq!(parse("/shrug"), Parsed::Command { command: SlashCommand::Shrug, len: 6 });
    }

    #[test]
    fn a_name_should_read_whatever_its_case() {
        assert_eq!(parse("/ME waves"), Parsed::Command { command: SlashCommand::Me, len: 3 });
    }

    #[test]
    fn an_unknown_name_should_be_reported() {
        assert_eq!(parse("/frobnicate now"), Parsed::Unknown("frobnicate".into()));
    }

    #[test]
    fn a_path_should_stay_a_message() {
        assert_eq!(parse("/usr/bin is full"), Parsed::Message);
    }

    #[test]
    fn two_slashes_should_send_one() {
        assert_eq!(parse("//me is not a command"), Parsed::Escaped);
    }

    #[test]
    fn text_should_stay_a_message() {
        assert_eq!(parse("hello /me"), Parsed::Message);
    }
}
