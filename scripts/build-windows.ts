#!/usr/bin/env bun
// Builds the Windows desktop binary. On Windows this is a plain cargo build; on Linux and
// macOS it cross-compiles to x86_64-pc-windows-msvc with cargo-xwin, which fetches the MSVC
// CRT and Windows SDK and drives clang-cl/lld-link (install LLVM, then `cargo install
// cargo-xwin`). Extra arguments pass through to cargo, e.g. `bun run build:windows --release`.
import { $ } from 'bun';

const target = 'x86_64-pc-windows-msvc';

const args = process.argv.slice(2);

await $`rustup target add ${target}`.quiet();

if (process.platform === 'win32') {
  await $`cargo build -p better-matrix-desktop --locked --target ${target} ${args}`;
} else {
  const xwin = await $`cargo xwin --version`.nothrow().quiet();

  if (xwin.exitCode !== 0) {
    console.error(
      'cargo-xwin is missing: run `cargo install --locked cargo-xwin` (needs clang and lld).',
    );
    process.exit(1);
  }

  await $`cargo xwin build -p better-matrix-desktop --locked --target ${target} ${args}`;
}

const profile = args.includes('--release') ? 'release' : 'debug';

console.log(`\nWindows binary: target/${target}/${profile}/better-matrix.exe`);
