#!/usr/bin/env bun
// Writes the third-party list the app shows under Settings → Info: every crate the desktop app
// is built from (normal and build dependencies, on every platform, without our own crates) and
// the files bundled into it, each with its version and license. `--check` fails when the list
// is out of date instead, so `bun run lint` catches a dependency change that wasn't listed.
import { $ } from 'bun';
import { readFile, writeFile } from 'node:fs/promises';
import { join } from 'node:path';

const root = join(import.meta.dir, '..');

const out = join(root, 'apps/desktop/assets/third-party.tsv');

const APP = 'better-matrix-desktop';

/** Files bundled into the binary that come with no crate of their own. */
const BUNDLED = [
  // Rune's `embedded-fonts`, the app's text face.
  { name: 'Inter (font)', version: '4.001', license: 'OFL-1.1' },
  { name: 'Noto Color Emoji (font)', version: '2.057', license: 'OFL-1.1' },
  { name: 'Noto Emoji (font)', version: '3.002', license: 'OFL-1.1' },
  // webrtc-sys's prebuilt Google libwebrtc (WEBRTC_TAG in webrtc-sys-build), for calls. Its
  // bundled parts (BoringSSL, libyuv, Opus, libvpx, …) carry permissive licenses of their own.
  { name: 'libwebrtc', version: 'a545de4', license: 'BSD-3-Clause' },
];

type Package = { id: string; name: string; version: string; license: string | null };

type Node = { id: string; deps: { pkg: string; dep_kinds: { kind: string | null }[] }[] };

const metadata = await $`cargo metadata --format-version 1 --locked`.cwd(root).quiet().json();

const packages = new Map<string, Package>(metadata.packages.map((p: Package) => [p.id, p]));

const nodes = new Map<string, Node>(metadata.resolve.nodes.map((n: Node) => [n.id, n]));

const ours = new Set<string>(metadata.workspace_members);

const app = metadata.workspace_members.find((id: string) => packages.get(id)?.name === APP);

// Everything the app is built from: dev-dependencies only build tests, so they don't ship.
const shipped = new Set<string>();

const stack = [app];

while (stack.length > 0) {
  const id = stack.pop();

  if (shipped.has(id)) continue;
  shipped.add(id);

  for (const dep of nodes.get(id)?.deps ?? []) {
    if (dep.dep_kinds.some((kind) => kind.kind === null || kind.kind === 'build'))
      stack.push(dep.pkg);
  }
}

const rows = shipped
  .values()
  .filter((id) => !ours.has(id))
  .map((id) => {
    const p = packages.get(id)!;

    if (!p.license) throw new Error(`${p.name} ${p.version} declares no license: list it by hand.`);

    // `MIT/Apache-2.0` is the old spelling of `MIT OR Apache-2.0`.
    return { name: p.name, version: p.version, license: p.license.split(/\s*\/\s*/).join(' OR ') };
  })
  .toArray()
  .concat(BUNDLED)
  .sort(
    (a, b) =>
      a.name.localeCompare(b.name) ||
      a.version.localeCompare(b.version, undefined, { numeric: true }),
  );

const text = rows.map((r) => `${r.name}\t${r.version}\t${r.license}`).join('\n') + '\n';

if (process.argv.includes('--check')) {
  const current = await readFile(out, 'utf8').catch(() => '');

  if (current !== text) {
    console.error('The third-party list is out of date: run `bun run third-party`.');
    process.exit(1);
  }
} else {
  await writeFile(out, text);
  console.log(`Listed ${rows.length} components in ${out}.`);
}
