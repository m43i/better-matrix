#!/usr/bin/env bun
// Installs Better Matrix for this user on Linux: a release build in ~/.local/bin, its icon in the
// hicolor theme, and a desktop entry, so launchers list it as "Better Matrix". Running it again
// updates the install in place. Accounts, history and keys live in the app's data folder and the
// keychain, which this never touches, so they carry over.
import { $ } from 'bun';
import { copyFile, mkdir, rename, writeFile } from 'node:fs/promises';
import { homedir } from 'node:os';
import { join } from 'node:path';

if (process.platform !== 'linux') {
  console.error('install:linux installs on Linux only.');
  process.exit(1);
}

/** The window's app id (`APP_ID` in apps/desktop/src/main.rs): the entry is named after it. */
const APP_ID = 'chat.bettermatrix.desktop';

const ICON = 'better-matrix';

const SIZES = [16, 24, 32, 48, 64, 128, 256, 512];

const root = join(import.meta.dir, '..');

const home = homedir();

const data = process.env.XDG_DATA_HOME ?? join(home, '.local/share');

const bin = join(home, '.local/bin');

const icons = join(data, 'icons/hicolor');

const svg = join(root, 'apps/desktop/assets/icon.svg');

await $`cargo build -p better-matrix-desktop --release --locked`.cwd(root);

// The binary: copied next to its place, then renamed over it, so a running copy keeps working.
await mkdir(bin, { recursive: true });

const target = join(bin, 'better-matrix');

await copyFile(join(root, 'target/release/better-matrix'), `${target}.new`);

await $`chmod 755 ${target}.new`;

await rename(`${target}.new`, target);

// The icon: the SVG for any size, PNGs for launchers that want them.
await mkdir(join(icons, 'scalable/apps'), { recursive: true });

await copyFile(svg, join(icons, 'scalable/apps', `${ICON}.svg`));

const rsvg = await $`which rsvg-convert`.nothrow().quiet();

if (rsvg.exitCode === 0) {
  for (const size of SIZES) {
    const dir = join(icons, `${size}x${size}/apps`);
    await mkdir(dir, { recursive: true });
    await $`rsvg-convert -w ${size} -h ${size} ${svg} -o ${join(dir, `${ICON}.png`)}`;
  }
} else {
  console.warn('rsvg-convert is missing: installed the SVG icon only.');
}

const entry = `[Desktop Entry]
Type=Application
Name=Better Matrix
GenericName=Matrix Client
Comment=Chat on Matrix with several accounts side by side
Exec=${target} %u
Icon=${ICON}
Terminal=false
Categories=Network;InstantMessaging;Chat;
Keywords=matrix;chat;messenger;
MimeType=x-scheme-handler/matrix;
StartupWMClass=${APP_ID}
StartupNotify=true
`;

const applications = join(data, 'applications');

await mkdir(applications, { recursive: true });

await writeFile(join(applications, `${APP_ID}.desktop`), entry);

// The entry claims `matrix:` links (MimeType); the database tells xdg-open.
await $`update-desktop-database ${applications}`.nothrow().quiet();

await $`gtk-update-icon-cache -f -t ${icons}`.nothrow().quiet();

console.log(`\nInstalled Better Matrix: ${target}`);
