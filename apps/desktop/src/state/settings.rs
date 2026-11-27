//! This device's settings: language, appearance, message layout and interface size. Account-wide settings (notification defaults)
//! live on the homeserver and arrive through the engine instead.

use crate::i18n::{Language, t};

/// Light, dark, or whatever the system uses.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Appearance {
    #[default]
    System,
    Light,
    Dark,
}

impl Appearance {
    pub const ALL: [Appearance; 3] = [Appearance::System, Appearance::Light, Appearance::Dark];

    pub fn label(self) -> &'static str {
        match self {
            Self::System => t!("appearance.system"),
            Self::Light => t!("appearance.light"),
            Self::Dark => t!("appearance.dark"),
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }

    pub fn from_id(id: &str) -> Self {
        Self::ALL.into_iter().find(|a| a.id() == id).unwrap_or_default()
    }
}

/// How conversations lay out their messages: classic rows, every message under its sender's
/// name as in Slack, or chat bubbles, other people's at the left by their picture and the
/// user's own at the right.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MessageLayout {
    #[default]
    Rows,
    Bubbles,
}

impl MessageLayout {
    pub const ALL: [MessageLayout; 2] = [MessageLayout::Rows, MessageLayout::Bubbles];

    pub fn label(self) -> &'static str {
        match self {
            Self::Rows => t!("message_layout.rows"),
            Self::Bubbles => t!("message_layout.bubbles"),
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            Self::Rows => t!("message_layout.rows_hint"),
            Self::Bubbles => t!("message_layout.bubbles_hint"),
        }
    }

    /// A stable name for keys and the prefs file, the same in every language.
    pub fn id(self) -> &'static str {
        match self {
            Self::Rows => "rows",
            Self::Bubbles => "bubbles",
        }
    }

    pub fn from_id(id: &str) -> Self {
        Self::ALL.into_iter().find(|layout| layout.id() == id).unwrap_or_default()
    }
}

/// What sending a message does with the text emoticons in it (`:)`, `:D`, `<3`): turns each
/// into its emoji (`matrix_core::emoticons`), or sends it as typed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Emoticons {
    #[default]
    AsEmoji,
    AsTyped,
}

impl Emoticons {
    pub const ALL: [Emoticons; 2] = [Emoticons::AsEmoji, Emoticons::AsTyped];

    pub fn label(self) -> &'static str {
        match self {
            Self::AsEmoji => t!("settings.emoticons_emoji"),
            Self::AsTyped => t!("settings.emoticons_typed"),
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            Self::AsEmoji => t!("settings.emoticons_emoji_hint"),
            Self::AsTyped => t!("settings.emoticons_typed_hint"),
        }
    }

    /// A stable name for keys and the prefs file, the same in every language.
    pub fn id(self) -> &'static str {
        match self {
            Self::AsEmoji => "emoji",
            Self::AsTyped => "typed",
        }
    }

    pub fn from_id(id: &str) -> Self {
        Self::ALL.into_iter().find(|emoticons| emoticons.id() == id).unwrap_or_default()
    }
}

/// How large the whole interface draws, as a browser's page zoom, in percent. It steps through
/// [`Zoom::LEVELS`]; 100% is the design size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Zoom(u16);

impl Zoom {
    pub const LEVELS: [u16; 9] = [67, 75, 80, 90, 100, 110, 125, 150, 175];
    pub const DEFAULT: Self = Self(100);

    /// The nearest level to `percent`.
    pub fn from_percent(percent: u16) -> Self {
        let nearest = Self::LEVELS.into_iter().min_by_key(|level| level.abs_diff(percent));
        Self(nearest.unwrap_or(Self::DEFAULT.0))
    }

    pub fn percent(self) -> u16 {
        self.0
    }

    /// The factor the view draws at.
    pub fn factor(self) -> f32 {
        f32::from(self.0) / 100.0
    }

    /// One level larger, or this one at the top.
    pub fn larger(self) -> Self {
        Self(Self::LEVELS.into_iter().find(|level| *level > self.0).unwrap_or(self.0))
    }

    /// One level smaller, or this one at the bottom.
    pub fn smaller(self) -> Self {
        Self(Self::LEVELS.into_iter().rev().find(|level| *level < self.0).unwrap_or(self.0))
    }
}

impl Default for Zoom {
    fn default() -> Self {
        Self::DEFAULT
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Settings {
    /// The account last on screen, restored at the next launch.
    pub last_account: Option<matrix_core::AccountId>,
    pub language: Language,
    pub appearance: Appearance,
    pub message_layout: MessageLayout,
    pub zoom: Zoom,
    /// Keep the app running in the system tray when its window closes.
    pub close_to_tray: bool,
    /// Play GIFs linked from Giphy, Tenor and Imgur. Loading them contacts those sites.
    pub show_link_gifs: bool,
    /// Links show a preview card (through the homeserver).
    pub show_link_previews: bool,
    /// Shared places show a map, drawn from OpenStreetMap's tiles: loading it contacts
    /// OpenStreetMap.
    pub show_maps: bool,
    /// Sent messages turn their text emoticons into emoji, or keep them as typed.
    pub emoticons: Emoticons,
    /// The microphone, speaker and camera for calls and voice messages, by name.
    pub devices: matrix_core::DeviceChoice,
    /// Ring aloud for an incoming call; silent, only the notification and the card show.
    pub ring_sound: bool,
    /// How fast voice memos and audio clips play.
    pub playback_speed: PlaybackSpeed,
    /// Timelines draw the events they otherwise leave out (developer tools).
    pub show_hidden_events: bool,
    /// Timelines draw profile and room-setting changes too, not only the room log
    /// (`RoomChange::log_only`).
    pub show_room_changes: bool,
}

/// How fast voice memos and audio clips play, at their own pitch. The speed button on a clip
/// steps through them, and the choice holds for every clip after.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PlaybackSpeed {
    #[default]
    Normal,
    Faster,
    Fastest,
}

impl PlaybackSpeed {
    /// How many times as fast as recorded.
    pub fn rate(self) -> f64 {
        match self {
            Self::Normal => 1.0,
            Self::Faster => 1.5,
            Self::Fastest => 2.0,
        }
    }

    /// The speed the button moves to: 1×, 1.5×, 2×, then 1× again.
    #[must_use]
    pub fn next(self) -> Self {
        match self {
            Self::Normal => Self::Faster,
            Self::Faster => Self::Fastest,
            Self::Fastest => Self::Normal,
        }
    }

    /// The rate in percent, as it is saved.
    pub fn percent(self) -> u16 {
        (self.rate() * 100.0) as u16
    }

    /// A saved rate; anything unknown plays as recorded.
    pub fn from_percent(percent: u16) -> Self {
        [Self::Faster, Self::Fastest].into_iter().find(|speed| speed.percent() == percent).unwrap_or_default()
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            last_account: None,
            language: Language::default(),
            appearance: Appearance::default(),
            message_layout: MessageLayout::default(),
            zoom: Zoom::default(),
            close_to_tray: true,
            show_link_gifs: true,
            show_link_previews: true,
            show_maps: true,
            emoticons: Emoticons::default(),
            devices: matrix_core::DeviceChoice::default(),
            ring_sound: true,
            playback_speed: PlaybackSpeed::default(),
            show_hidden_events: false,
            show_room_changes: false,
        }
    }
}

impl Settings {
    /// English, whatever the system says: headless renders and tests stay the same everywhere.
    pub fn fixed_english() -> Self {
        Self { language: Language::Locale("en"), ..Self::default() }
    }
}

/// A section of the settings modal. New sections are new variants.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SettingsSection {
    #[default]
    General,
    /// The theme, the interface size and how messages lay out.
    Appearance,
    /// Composition, previews, media and the room events shown in conversations.
    MessagesMedia,
    /// The active account's name, picture and contact details.
    Profile,
    Notifications,
    /// The active account's safety: ignored people, media and strangers' invitations.
    Safety,
    /// The active account: its encryption and recovery.
    Account,
    /// The active account's signed-in sessions.
    Sessions,
    /// The microphone, speaker and camera for calls and voice messages.
    VoiceVideo,
    /// The app's version and the third-party software it is built from.
    Info,
}

impl SettingsSection {
    pub const PREFERENCES: [SettingsSection; 5] = [
        SettingsSection::General,
        SettingsSection::Appearance,
        SettingsSection::MessagesMedia,
        SettingsSection::Notifications,
        SettingsSection::VoiceVideo,
    ];

    pub const ACCOUNT: [SettingsSection; 4] =
        [SettingsSection::Profile, SettingsSection::Safety, SettingsSection::Account, SettingsSection::Sessions];

    pub fn label(self) -> &'static str {
        match self {
            Self::General => t!("settings.general"),
            Self::Appearance => t!("settings.appearance"),
            Self::MessagesMedia => t!("settings.messages_media"),
            Self::Profile => t!("settings.profile"),
            Self::Notifications => t!("settings.notifications"),
            Self::Safety => t!("settings.safety"),
            Self::Account => t!("settings.account"),
            Self::Sessions => t!("settings.sessions"),
            Self::VoiceVideo => t!("settings.voice_video"),
            Self::Info => t!("settings.info"),
        }
    }

    /// A stable name for keys, the same in every language.
    pub fn id(self) -> &'static str {
        match self {
            Self::General => "general",
            Self::Appearance => "appearance",
            Self::MessagesMedia => "messages-media",
            Self::Profile => "profile",
            Self::Notifications => "notifications",
            Self::Safety => "safety",
            Self::Account => "account",
            Self::Sessions => "sessions",
            Self::VoiceVideo => "voice-video",
            Self::Info => "info",
        }
    }
}

#[cfg(test)]
mod tests;
