//! The app's icons: 24-point stroke drawings in `currentColor`, tinted by the element's colour.

use rune::Icon;

macro_rules! icons {
    ($($name:ident = $id:literal: $body:literal;)*) => {$(
        pub static $name: Icon = Icon {
            name: concat!("bm-", $id),
            view: 24.0,
            body: concat!(
                r#"<g fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">"#,
                $body,
                "</g>"
            ),
        };
    )*};
}

icons! {
    PIN = "pin": r#"<path d="M16 3H8l1 6-4 4v2h6v7h2v-7h6v-2l-4-4z"/>"#;
    COPY = "copy": r#"<rect x="8" y="8" width="12" height="13" rx="2"/><path d="M16 8V3H3v13h5"/>"#;
    FORWARD = "forward": r#"<path d="m15 17 5-5-5-5M20 12H9a5 5 0 0 0-5 5v2"/>"#;
    LAYOUT = "layout": r#"<rect x="3" y="3" width="18" height="18" rx="2"/><path d="M15 3v18M15 12h6"/>"#;
    SIDEBAR = "sidebar": r#"<rect x="3" y="3" width="18" height="18" rx="2"/><path d="M9 3v18"/>"#;
    BOLD = "bold": r#"<path d="M7 5h6a3.5 3.5 0 0 1 0 7H7zM7 12h7a3.5 3.5 0 0 1 0 7H7z"/>"#;
    ITALIC = "italic": r#"<path d="M19 4h-9M14 20H5M15 4 9 20"/>"#;
    STRIKE = "strike": r#"<path d="M16 6.5C15 5 13.6 4.5 12 4.5c-2.5 0-4.5 1.3-4.5 3.4 0 1.5.8 2.5 2.4 3.1M4 12h16M8 17c1 1.6 2.4 2.5 4.3 2.5 2.6 0 4.7-1.3 4.7-3.6 0-.7-.2-1.3-.5-1.9"/>"#;
    CODE = "code": r#"<path d="m16 18 6-6-6-6M8 6l-6 6 6 6"/>"#;
    LINK = "link": r#"<path d="M10 13a5 5 0 0 0 7.5.5l3-3a5 5 0 0 0-7-7l-1.7 1.7M14 11a5 5 0 0 0-7.5-.5l-3 3a5 5 0 0 0 7 7l1.7-1.7"/>"#;
    BULLETS = "bullets": r#"<path d="M9 6h11M9 12h11M9 18h11"/><circle cx="4.5" cy="6" r=".6"/><circle cx="4.5" cy="12" r=".6"/><circle cx="4.5" cy="18" r=".6"/>"#;
    NUMBERS = "numbers": r#"<path d="M10 6h10M10 12h10M10 18h10M4 4h1.5v5M4 9h3M4 15.2c.3-.7 1-1.2 1.8-1.2 1 0 1.7.7 1.7 1.5 0 1.3-3.5 2.4-3.5 4.5h3.5"/>"#;
    QUOTE = "quote": r#"<path d="M5 5v14M10 8h9M10 12h9M10 16h6"/>"#;
    CODE_BLOCK = "code-block": r#"<rect x="3" y="4" width="18" height="16" rx="2"/><path d="m10 10-2 2 2 2M14 10l2 2-2 2"/>"#;
    PAPERCLIP = "paperclip": r#"<path d="m20.5 11.5-8.4 8.4a5.5 5.5 0 0 1-7.8-7.8l8.4-8.4a3.7 3.7 0 0 1 5.2 5.2l-8.4 8.4a1.8 1.8 0 0 1-2.6-2.6l7.8-7.8"/>"#;
    SEND = "send": r#"<path d="M21 3 10 14M21 3l-6.5 18-4.5-7-7-4.5z"/>"#;
    CLOSE = "close": r#"<path d="M6 6l12 12M18 6 6 18"/>"#;
    FILE = "file": r#"<path d="M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8zM14 3v5h5"/>"#;
    IMAGE = "image": r#"<rect x="3" y="3" width="18" height="18" rx="2"/><circle cx="9" cy="9" r="2"/><path d="m21 15-5-5L5 21"/>"#;
    VIDEO = "video": r#"<rect x="2" y="6" width="14" height="12" rx="2"/><path d="m16 10 6-3v10l-6-3z"/>"#;
    AUDIO = "audio": r#"<path d="M9 18V5l12-2v13"/><circle cx="6" cy="18" r="3"/><circle cx="18" cy="16" r="3"/>"#;
    HOME = "home": r#"<path d="M3 10.5 12 3l9 7.5V20a1 1 0 0 1-1 1h-5v-6H9v6H4a1 1 0 0 1-1-1z"/>"#;
    PLUS = "plus": r#"<path d="M12 5v14M5 12h14"/>"#;
    CHECK = "check": r#"<path d="M20 6 9 17l-5-5"/>"#;
    SIGN_OUT = "sign-out": r#"<path d="M9 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h4M16 17l5-5-5-5M21 12H9"/>"#;
    SETTINGS = "settings": r#"<circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.7 1.7 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.8-.3 1.7 1.7 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-1.1-1.5 1.7 1.7 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0 .3-1.8 1.7 1.7 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.5-1.1 1.7 1.7 0 0 0-.3-1.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.8.3H9a1.7 1.7 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.8V9a1.7 1.7 0 0 0 1.5 1H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1z"/>"#;
    LANGUAGE = "language": r#"<path d="m5 8 6 6M4 14l6-6 2-3M2 5h12M7 2h1M22 22l-5-10-5 10M14 18h6"/>"#;
    SUN_MOON = "appearance": r#"<circle cx="12" cy="12" r="4"/><path d="M12 2v2M12 20v2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M2 12h2M20 12h2M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4"/>"#;
    BELL = "bell": r#"<path d="M6 8a6 6 0 0 1 12 0c0 7 3 9 3 9H3s3-2 3-9M10.3 21a1.9 1.9 0 0 0 3.4 0"/>"#;
    BELL_OFF = "bell-off": r#"<path d="M8.7 3A6 6 0 0 1 18 8c0 2.7.4 4.6 1 6M17 17H3s3-2 3-9c0-.7.1-1.4.4-2M10.3 21a1.9 1.9 0 0 0 3.4 0M2 2l20 20"/>"#;
    AT = "at": r#"<circle cx="12" cy="12" r="4"/><path d="M16 8v5a3 3 0 0 0 6 0v-1a10 10 0 1 0-4 8"/>"#;
    CHEVRON_LEFT = "chevron-left": r#"<path d="m15 18-6-6 6-6"/>"#;
    CHEVRON_RIGHT = "chevron-right": r#"<path d="m9 18 6-6-6-6"/>"#;
    LEAVE = "leave": r#"<path d="M14 3h5a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2h-5M10 17l-5-5 5-5M5 12h11"/>"#;
    TRASH = "trash": r#"<path d="M3 6h18M8 6V4a1 1 0 0 1 1-1h6a1 1 0 0 1 1 1v2M19 6l-1 14a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2L5 6M10 11v6M14 11v6"/>"#;
    EYE_OFF = "eye-off": r#"<path d="M10.7 5.1A10 10 0 0 1 12 5c7 0 10 7 10 7a13 13 0 0 1-1.7 2.7M6.6 6.6A13.5 13.5 0 0 0 2 12s3 7 10 7a9.7 9.7 0 0 0 5.4-1.6M2 2l20 20M9.9 9.9a3 3 0 0 0 4.2 4.2"/>"#;
    SMILE = "smile": r#"<circle cx="12" cy="12" r="10"/><path d="M8 14s1.5 2 4 2 4-2 4-2M9 9h.01M15 9h.01"/>"#;
    REPLY = "reply": r#"<path d="m9 17-5-5 5-5M4 12h11a5 5 0 0 1 5 5v2"/>"#;
    THREAD = "thread": r#"<path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2zM8 9h8M8 13h5"/>"#;
    LOCK = "lock": r#"<rect x="4" y="11" width="16" height="10" rx="2"/><path d="M8 11V7a4 4 0 0 1 8 0v4"/>"#;
    ALERT = "alert": r#"<path d="M10.3 3.9 1.8 18a2 2 0 0 0 1.7 3h17a2 2 0 0 0 1.7-3L13.7 3.9a2 2 0 0 0-3.4 0zM12 9v4M12 17h.01"/>"#;
    USER = "user": r#"<circle cx="12" cy="8" r="4"/><path d="M4 21a8 8 0 0 1 16 0"/>"#;
    MAIL = "mail": r#"<rect x="3" y="5" width="18" height="14" rx="2"/><path d="m3 7 9 6 9-6"/>"#;
    PHONE = "phone": r#"<rect x="6" y="2" width="12" height="20" rx="2"/><path d="M11 18h2"/>"#;
    EXTERNAL = "external": r#"<path d="M14 4h6v6M20 4l-9 9M18 14v5a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V7a1 1 0 0 1 1-1h5"/>"#;
    ZOOM = "zoom": r#"<circle cx="11" cy="11" r="7"/><path d="m20 20-3.5-3.5M8 11h6M11 8v6"/>"#;
    // A speech bubble of halftone dots, three typing dots at its heart; the app icon in one colour.
    LOGO = "logo": r#"<g fill="currentColor" stroke="none"><circle cx="6.20" cy="4.98" r="0.59"/><circle cx="9.10" cy="4.98" r="0.67"/><circle cx="12.00" cy="4.98" r="0.70"/><circle cx="14.90" cy="4.98" r="0.67"/><circle cx="17.80" cy="4.98" r="0.59"/><circle cx="4.75" cy="7.49" r="0.62"/><circle cx="7.65" cy="7.49" r="0.74"/><circle cx="10.55" cy="7.49" r="0.82"/><circle cx="13.45" cy="7.49" r="0.82"/><circle cx="16.35" cy="7.49" r="0.74"/><circle cx="19.25" cy="7.49" r="0.62"/><circle cx="3.30" cy="10.00" r="0.58"/><circle cx="6.20" cy="10.00" r="0.72"/><circle cx="9.10" cy="10.00" r="1.25"/><circle cx="12.00" cy="10.00" r="1.25"/><circle cx="14.90" cy="10.00" r="1.25"/><circle cx="17.80" cy="10.00" r="0.72"/><circle cx="20.70" cy="10.00" r="0.58"/><circle cx="4.75" cy="12.51" r="0.62"/><circle cx="7.65" cy="12.51" r="0.74"/><circle cx="10.55" cy="12.51" r="0.82"/><circle cx="13.45" cy="12.51" r="0.82"/><circle cx="16.35" cy="12.51" r="0.74"/><circle cx="19.25" cy="12.51" r="0.62"/><circle cx="6.20" cy="15.02" r="0.59"/><circle cx="9.10" cy="15.02" r="0.67"/><circle cx="12.00" cy="15.02" r="0.70"/><circle cx="14.90" cy="15.02" r="0.67"/><circle cx="17.80" cy="15.02" r="0.59"/><circle cx="6.9" cy="17.4" r="1.05"/><circle cx="5.3" cy="19.3" r=".7"/></g>"#;
    DOWNLOAD = "download": r#"<path d="M12 4v11M7 10l5 5 5-5M5 20h14"/>"#;
    MIC = "mic": r#"<rect x="9" y="3" width="6" height="11" rx="3"/><path d="M5 11a7 7 0 0 0 14 0M12 18v3"/>"#;
    MIC_OFF = "mic-off": r#"<rect x="9" y="3" width="6" height="11" rx="3"/><path d="M5 11a7 7 0 0 0 14 0M12 18v3M3 3l18 18"/>"#;
    VIDEO_OFF = "video-off": r#"<rect x="2" y="6" width="14" height="12" rx="2"/><path d="m16 10 6-3v10l-6-3M3 3l18 18"/>"#;
    SCREEN_SHARE = "screen-share": r#"<rect x="2" y="4" width="20" height="13" rx="2"/><path d="M8 21h8M12 17v4M12 13.5V8M9.5 10.5 12 8l2.5 2.5"/>"#;
    CALL = "call": r#"<path d="M5 3.5h3.2l1.8 4.6-2.3 1.5a11 11 0 0 0 6.7 6.7l1.5-2.3 4.6 1.8V19a2 2 0 0 1-2 2A16.5 16.5 0 0 1 3 5.5a2 2 0 0 1 2-2z"/>"#;
    HANG_UP = "hang-up": r#"<g transform="rotate(135 12 12)"><path d="M5 3.5h3.2l1.8 4.6-2.3 1.5a11 11 0 0 0 6.7 6.7l1.5-2.3 4.6 1.8V19a2 2 0 0 1-2 2A16.5 16.5 0 0 1 3 5.5a2 2 0 0 1 2-2z"/></g>"#;
    SEARCH = "search": r#"<circle cx="11" cy="11" r="7"/><path d="m20 20-3.5-3.5"/>"#;
    INFO = "info": r#"<circle cx="12" cy="12" r="9"/><path d="M12 11v5M12 8h.01"/>"#;
    CHEVRON_DOWN = "chevron-down": r#"<path d="m6 9 6 6 6-6"/>"#;
    CHEVRON_UP = "chevron-up": r#"<path d="m6 15 6-6 6 6"/>"#;
    POLL = "poll": r#"<path d="M4 20h16M7 16v-5M12 16V5M17 16v-8"/>"#;
    USER_PLUS = "user-plus": r#"<circle cx="9" cy="8" r="4"/><path d="M2 21a7 7 0 0 1 14 0M19 8v6M16 11h6"/>"#;
    VERIFIED = "verified": r#"<path d="M12 2.5l2.3 1.7 2.9-.1 1 2.7 2.3 1.8-.8 2.8.8 2.8-2.3 1.8-1 2.7-2.9-.1L12 21.5l-2.3-1.7-2.9.1-1-2.7-2.3-1.8.8-2.8-.8-2.8 2.3-1.8 1-2.7 2.9.1z"/><path d="m8.5 12 2.4 2.4 4.6-4.8"/>"#;
    SPEAKER = "speaker": r#"<path d="M11 5 6 9H2v6h4l5 4z"/><path d="M15.5 8.5a5 5 0 0 1 0 7M19 5a10 10 0 0 1 0 14"/>"#;
    PACKAGE = "package": r#"<path d="m21 7.5-9-5-9 5v9l9 5 9-5z"/><path d="m3 7.5 9 5 9-5M12 12.5v9"/>"#;
    PLAY = "play": r#"<path d="M8 5.5v13a1 1 0 0 0 1.5.9l10.4-6.5a1 1 0 0 0 0-1.8L9.5 4.6A1 1 0 0 0 8 5.5z"/>"#;
    PAUSE = "pause": r#"<path d="M8 5v14M16 5v14"/>"#;
    CHAT = "chat": r#"<path d="M20 12a8 8 0 0 1-11.6 7.1L4 20l.9-4.4A8 8 0 1 1 20 12z"/><path d="M12 9v6M9 12h6"/>"#;
    KEY = "key": r#"<circle cx="7.5" cy="15.5" r="4.5"/><path d="m10.7 12.3 9.8-9.8M17 5l3 3M14.5 7.5l2 2"/>"#;
    SHIELD = "shield": r#"<path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z"/><path d="m9 12 2 2 4-4"/>"#;
    LAPTOP = "laptop": r#"<rect x="4" y="4" width="16" height="11" rx="2"/><path d="M2 20h20"/>"#;
    MORE = "more": r#"<circle cx="12" cy="5" r="1"/><circle cx="12" cy="12" r="1"/><circle cx="12" cy="19" r="1"/>"#;
    PENCIL = "pencil": r#"<path d="M17 3a2.8 2.8 0 1 1 4 4L7.5 20.5 2 22l1.5-5.5z"/>"#;
    GLOBE = "globe": r#"<circle cx="12" cy="12" r="10"/><path d="M2 12h20M12 2a15 15 0 0 1 0 20M12 2a15 15 0 0 0 0 20"/>"#;
}
