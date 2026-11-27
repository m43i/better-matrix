#!/usr/bin/env bun
// Builds the Windows desktop binary. Requires LLVM for llvm-ar. On Linux and macOS, cargo-xwin
// also uses clang-cl and lld-link with the MSVC CRT and Windows SDK it fetches.
// Extra arguments pass through to Cargo, e.g. `bun run build:windows --release`.
import { $ } from 'bun';
import { mkdir, mkdtemp, rename, rm } from 'node:fs/promises';
import { join, resolve } from 'node:path';

const target = 'x86_64-pc-windows-msvc';

const targetDir = resolve(process.env.CARGO_TARGET_DIR ?? 'target');

const maplibre = {
  tag: 'v0.202610.3',
  archive: 'maplibre-native-c-windows-x64-vulkan',
  sha256: 'e54bf82ea7be381ba08abd3e0e21a5c9d8949d398603941a37b67e33cc9fd459',
};

async function prepareMapLibre() {
  const prefix = join(targetDir, `maplibre-${maplibre.sha256}`);
  const archivePath = join('lib', 'maplibre-native-c-static.lib');

  if (await Bun.file(join(prefix, archivePath)).exists()) return prefix;

  await mkdir(targetDir, { recursive: true });
  const staging = await mkdtemp(join(targetDir, 'maplibre-'));

  try {
    const fileName = `${maplibre.archive}.tar.gz`;
    const url = `https://github.com/maplibre/maplibre-native-ffi/releases/download/${maplibre.tag}/${fileName}`;
    const response = await fetch(url, { signal: AbortSignal.timeout(300_000) });

    if (!response.ok)
      throw new Error(`MapLibre archive download failed with HTTP ${response.status}`);

    const bytes = await response.arrayBuffer();
    const sha256 = new Bun.CryptoHasher('sha256').update(bytes).digest('hex');

    if (sha256 !== maplibre.sha256) throw new Error(`Checksum mismatch for ${fileName}`);

    const tarball = join(staging, fileName);
    await Bun.write(tarball, bytes);
    await $`tar -xzf ${tarball} -C ${staging}`;

    // Link MapLibre against the same bundled SQLite that matrix-rust-sdk uses.
    const unpacked = join(staging, maplibre.archive);
    const archive = join(unpacked, archivePath);
    const isSqlite = (member: string) => /[/\\]sqlite3\.c\.obj$/.test(member);
    const members = (await $`llvm-ar t ${archive}`.text()).split(/\r?\n/);
    const sqlite = members.find(isSqlite);

    if (!sqlite) throw new Error('MapLibre archive lacks its SQLite object');
    await $`llvm-ar d ${archive} ${sqlite}`;

    if ((await $`llvm-ar t ${archive}`.text()).split(/\r?\n/).some(isSqlite)) {
      throw new Error('MapLibre archive still contains a SQLite object');
    }

    try {
      await rename(unpacked, prefix);
    } catch (error) {
      if (!(await Bun.file(join(prefix, archivePath)).exists())) throw error;
    }

    return prefix;
  } finally {
    await rm(staging, { recursive: true, force: true });
  }
}

const args = process.argv.slice(2);

await $`rustup target add ${target}`.quiet();

const crossCompile = process.platform !== 'win32';

if (crossCompile) {
  const xwin = await $`cargo xwin --version`.nothrow().quiet();

  if (xwin.exitCode !== 0) {
    console.error(
      'cargo-xwin is missing: run `cargo install --locked cargo-xwin` (needs clang and lld).',
    );
    process.exit(1);
  }
}

const env = {
  ...process.env,
  MAPLIBRE_NATIVE_FFI_LOCAL_BUILD:
    process.env.MAPLIBRE_NATIVE_FFI_LOCAL_BUILD ?? (await prepareMapLibre()),
};

const build = crossCompile ? ['xwin', 'build'] : ['build'];

await $`cargo ${build} -p better-matrix-desktop --locked --target ${target} ${args}`.env(env);

const profile = args.includes('--release') ? 'release' : 'debug';

console.log(`\nWindows binary: ${join(targetDir, target, profile, 'better-matrix.exe')}`);
