// The desktop's palette (apps/desktop/src/ui/theme.rs), as sRGB: the same roles, so both apps
// share one design. Views read roles, never raw colours.

export interface Palette {
  railBg: string;
  sidebarBg: string;
  sidebarText: string;
  sidebarTextMuted: string;
  sidebarHover: string;
  sidebarSelected: string;
  sidebarSelectedText: string;
  sidebarDivider: string;
  bg: string;
  surface: string;
  text: string;
  textMuted: string;
  divider: string;
  hover: string;
  /** Behind other people's messages in chat bubbles. */
  bubble: string;
  /** Behind the user's own messages in chat bubbles. */
  bubbleOwn: string;
  /** The time and status inside a bubble: muted, yet clear on either bubble. */
  bubbleMeta: string;
  accent: string;
  onAccent: string;
  mention: string;
  danger: string;
  success: string;
  warning: string;
  /** Dimmed backdrop behind dialogs and the attachment preview. */
  scrim: string;
  dark: boolean;
}

const common = {
  accent: '#3b82f6',
  onAccent: '#ffffff',
  mention: '#ef4444',
  danger: '#ef4444',
  success: '#45b164',
  warning: '#efa831',
};

export const dark: Palette = {
  railBg: '#0f0f0f',
  sidebarBg: '#171717',
  sidebarText: '#e5e5e5',
  sidebarTextMuted: '#a3a3a3',
  sidebarHover: '#ffffff0f',
  sidebarSelected: '#1e3a8a',
  sidebarSelectedText: '#bfdbfe',
  sidebarDivider: '#404040',
  bg: '#171717',
  surface: '#262626',
  text: '#e5e5e5',
  textMuted: '#a3a3a3',
  divider: '#404040',
  hover: '#ffffff0d',
  bubble: '#2b2b2b',
  bubbleOwn: '#1e2638',
  bubbleMeta: '#a3a3a3',
  scrim: '#000000b3',
  dark: true,
  ...common,
};

export const light: Palette = {
  railBg: '#f3f4f6',
  sidebarBg: '#f9fafb',
  sidebarText: '#333333',
  sidebarTextMuted: '#6b7280',
  sidebarHover: '#0000000d',
  sidebarSelected: '#e0f2fe',
  sidebarSelectedText: '#1e3a8a',
  sidebarDivider: '#e5e7eb',
  bg: '#ffffff',
  surface: '#f9fafb',
  text: '#333333',
  textMuted: '#6b7280',
  divider: '#e5e7eb',
  hover: '#0000000a',
  bubble: '#eef0f3',
  bubbleOwn: '#dfe9fb',
  bubbleMeta: '#4b5563',
  scrim: '#00000080',
  dark: false,
  ...common,
};

/** A stable colour per Matrix ID, for avatars and sender names: the desktop's `identity_color`. */
export function identityColor(id: string): string {
  // FNV-1a over UTF-8 bytes, as the desktop hashes, so a person has one colour everywhere.
  let hash = 0x811c9dc5;

  for (const byte of new TextEncoder().encode(id)) {
    hash = Math.imul(hash ^ byte, 0x01000193) >>> 0;
  }

  return oklchToHex(0.62, 0.13, hash % 360);
}

/** OKLCH to `#rrggbb`, clamped to sRGB. */
export function oklchToHex(l: number, c: number, hue: number): string {
  const h = (hue * Math.PI) / 180;
  const a = c * Math.cos(h);
  const b = c * Math.sin(h);
  const l_ = l + 0.3963377774 * a + 0.2158037573 * b;
  const m_ = l - 0.1055613458 * a - 0.0638541728 * b;
  const s_ = l - 0.0894841775 * a - 1.291485548 * b;
  const [L, M, S] = [l_ ** 3, m_ ** 3, s_ ** 3];

  const linear = [
    4.0767416621 * L - 3.3077115913 * M + 0.2309699292 * S,
    -1.2684380046 * L + 2.6097574011 * M - 0.3413193965 * S,
    -0.0041960863 * L - 0.7034186147 * M + 1.707614701 * S,
  ];

  return (
    '#' +
    linear
      .map((x) => {
        const v = Math.min(1, Math.max(0, x));
        const srgb = v <= 0.0031308 ? 12.92 * v : 1.055 * v ** (1 / 2.4) - 0.055;

        return Math.round(srgb * 255)
          .toString(16)
          .padStart(2, '0');
      })
      .join('')
  );
}

/** The desktop's type scale, at the same sizes: phones read them at arm's length too. */
export const font = {
  workspace: { fontSize: 17, fontWeight: '700' },
  header: { fontSize: 16, fontWeight: '700' },
  body: { fontSize: 15, fontWeight: '400', lineHeight: 22 },
  bodyStrong: { fontSize: 15, fontWeight: '700' },
  nav: { fontSize: 15, fontWeight: '400' },
  navStrong: { fontSize: 15, fontWeight: '700' },
  section: { fontSize: 13, fontWeight: '600' },
  meta: { fontSize: 12, fontWeight: '400' },
  badge: { fontSize: 11, fontWeight: '700' },
  title: { fontSize: 28, fontWeight: '700' },
} as const;
