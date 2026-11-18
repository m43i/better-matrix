//! Semantic colours for the whole app. Views read roles (`sidebar_text`, `unread`, …), never
//! raw colours, so a new theme or appearance is one constructor here.

use rune::{Color, oklch, oklcha};

/// Every colour role the UI paints with.
#[derive(Clone, Copy, Debug)]
pub struct Palette {
    pub rail_bg: Color,
    pub sidebar_bg: Color,
    pub sidebar_text: Color,
    pub sidebar_text_muted: Color,
    pub sidebar_hover: Color,
    pub sidebar_selected: Color,
    pub sidebar_selected_text: Color,
    /// Behind a room or direct chat with something new in it.
    pub sidebar_unread: Color,
    pub sidebar_divider: Color,
    pub bg: Color,
    pub surface: Color,
    pub text: Color,
    pub text_muted: Color,
    pub divider: Color,
    pub hover: Color,
    pub accent: Color,
    pub on_accent: Color,
    pub mention: Color,
    pub danger: Color,
    pub success: Color,
    pub warning: Color,
}

/// The app's palette, light or dark: the design tokens (`--background`, `--primary`,
/// `--sidebar-accent`, …) mapped onto the roles views paint with. The sidebar follows the
/// appearance like the content pane; the rail sits a step darker than the sidebar.
pub fn palette(dark: bool) -> Palette {
    let primary = oklch(0.6231, 0.1880, 259.8145);
    let destructive = oklch(0.6368, 0.2078, 25.3313);
    let (sidebar, content) = if dark {
        (
            Sidebar {
                rail_bg: oklch(0.17, 0.0, 0.0),
                sidebar_bg: oklch(0.2046, 0.0, 0.0),
                sidebar_text: oklch(0.9219, 0.0, 0.0),
                sidebar_text_muted: oklch(0.7155, 0.0, 0.0),
                sidebar_hover: oklcha(1.0, 0.0, 0.0, 0.06),
                sidebar_selected: oklch(0.3791, 0.1378, 265.5222),
                sidebar_unread: oklcha(0.6231, 0.1880, 259.8145, 0.18),
                sidebar_selected_text: oklch(0.8823, 0.0571, 254.1284),
                sidebar_divider: oklch(0.3715, 0.0, 0.0),
            },
            Content {
                bg: oklch(0.2046, 0.0, 0.0),
                surface: oklch(0.2686, 0.0, 0.0),
                text: oklch(0.9219, 0.0, 0.0),
                text_muted: oklch(0.7155, 0.0, 0.0),
                divider: oklch(0.3715, 0.0, 0.0),
                hover: oklcha(1.0, 0.0, 0.0, 0.05),
            },
        )
    } else {
        (
            Sidebar {
                rail_bg: oklch(0.9670, 0.0029, 264.5419),
                sidebar_bg: oklch(0.9846, 0.0017, 247.8389),
                sidebar_text: oklch(0.3211, 0.0, 0.0),
                sidebar_text_muted: oklch(0.5510, 0.0234, 264.3637),
                sidebar_hover: oklcha(0.0, 0.0, 0.0, 0.05),
                sidebar_selected: oklch(0.9514, 0.0250, 236.8242),
                sidebar_unread: oklcha(0.6231, 0.1880, 259.8145, 0.12),
                sidebar_selected_text: oklch(0.3791, 0.1378, 265.5222),
                sidebar_divider: oklch(0.9276, 0.0058, 264.5313),
            },
            Content {
                bg: oklch(1.0, 0.0, 0.0),
                surface: oklch(0.9846, 0.0017, 247.8389),
                text: oklch(0.3211, 0.0, 0.0),
                text_muted: oklch(0.5510, 0.0234, 264.3637),
                divider: oklch(0.9276, 0.0058, 264.5313),
                hover: oklcha(0.0, 0.0, 0.0, 0.04),
            },
        )
    };
    Palette {
        rail_bg: sidebar.rail_bg,
        sidebar_bg: sidebar.sidebar_bg,
        sidebar_text: sidebar.sidebar_text,
        sidebar_text_muted: sidebar.sidebar_text_muted,
        sidebar_hover: sidebar.sidebar_hover,
        sidebar_selected: sidebar.sidebar_selected,
        sidebar_unread: sidebar.sidebar_unread,
        sidebar_selected_text: sidebar.sidebar_selected_text,
        sidebar_divider: sidebar.sidebar_divider,
        bg: content.bg,
        surface: content.surface,
        text: content.text,
        text_muted: content.text_muted,
        divider: content.divider,
        hover: content.hover,
        accent: primary,
        on_accent: oklch(1.0, 0.0, 0.0),
        mention: destructive,
        danger: destructive,
        success: oklch(0.68, 0.15, 150.0),
        warning: oklch(0.78, 0.15, 75.0),
    }
}

struct Sidebar {
    rail_bg: Color,
    sidebar_bg: Color,
    sidebar_text: Color,
    sidebar_text_muted: Color,
    sidebar_hover: Color,
    sidebar_selected: Color,
    sidebar_unread: Color,
    sidebar_selected_text: Color,
    sidebar_divider: Color,
}

struct Content {
    bg: Color,
    surface: Color,
    text: Color,
    text_muted: Color,
    divider: Color,
    hover: Color,
}

/// A stable colour per Matrix ID, for avatars and sender names.
pub fn identity_color(id: &str) -> Color {
    // FNV-1a: stable across runs and platforms, unlike `DefaultHasher`.
    let hash = id.bytes().fold(0x811c_9dc5_u32, |h, b| (h ^ u32::from(b)).wrapping_mul(0x0100_0193));
    let hue = (hash % 360) as f32;
    oklch(0.62, 0.13, hue)
}

/// Type scale.
/// The height of every pane's header (sidebar, room, thread, overview), so their dividers meet
/// in one line across the window.
pub const HEADER_HEIGHT: f32 = 56.0;

pub mod font {
    use rune::TextStyle;

    pub const WORKSPACE: TextStyle = TextStyle::new(17.0, 700.0);
    pub const HEADER: TextStyle = TextStyle::new(16.0, 700.0);
    pub const BODY: TextStyle = TextStyle::new(15.0, 400.0).lh(22.0);
    pub const BODY_STRONG: TextStyle = TextStyle::new(15.0, 700.0);
    pub const NAV: TextStyle = TextStyle::new(15.0, 400.0);
    pub const NAV_STRONG: TextStyle = TextStyle::new(15.0, 700.0);
    pub const SECTION: TextStyle = TextStyle::new(13.0, 600.0);
    /// A section title with something new in it.
    pub const SECTION_STRONG: TextStyle = TextStyle::new(13.0, 700.0);
    pub const META: TextStyle = TextStyle::new(12.0, 400.0);
    pub const BADGE: TextStyle = TextStyle::new(11.0, 700.0);
    pub const TITLE: TextStyle = TextStyle::new(28.0, 700.0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_color_should_be_stable_per_id() {
        assert_eq!(identity_color("@a:example.org"), identity_color("@a:example.org"));
    }

    #[test]
    fn identity_color_should_differ_between_ids() {
        assert_ne!(identity_color("@a:example.org"), identity_color("@b:example.org"));
    }
}
