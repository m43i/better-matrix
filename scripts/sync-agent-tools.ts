#!/usr/bin/env bun
// Refreshes .agent-tools/ (gitignored): local reference material for coding agents.
// Rune: its examples and crate READMEs, at exactly the rev the workspace pins in Cargo.toml,
// so agents read docs that match the code they compile against.
import { $ } from 'bun';
import { mkdtemp, rm, cp, mkdir, readdir } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const root = join(import.meta.dir, '..');

const cargoToml = await Bun.file(join(root, 'Cargo.toml')).text();

const rev = cargoToml.match(/^rune = \{ git = "[^"]+", rev = "([0-9a-f]{40})" \}/m)?.[1];

if (!rev)
  throw new Error('No pinned `rune = { git, rev }` in [workspace.dependencies] of Cargo.toml');

const checkout = await mkdtemp(join(tmpdir(), 'rune-'));

try {
  await $`git init -q ${checkout}`;
  await $`git -C ${checkout} fetch -q --depth 1 https://github.com/anomalyco/rune ${rev}`;
  await $`git -C ${checkout} checkout -q FETCH_HEAD`;

  const dest = join(root, '.agent-tools', 'rune');
  await rm(dest, { recursive: true, force: true });
  await mkdir(join(dest, 'docs'), { recursive: true });
  await cp(join(checkout, 'examples'), join(dest, 'examples'), { recursive: true });

  for (const crate of await readdir(join(checkout, 'crates'))) {
    const readme = Bun.file(join(checkout, 'crates', crate, 'README.md'));

    if (await readme.exists()) await Bun.write(join(dest, 'docs', `${crate}.md`), readme);
  }

  for (const file of ['README.md', 'AGENTS.md']) {
    await Bun.write(join(dest, 'docs', file), Bun.file(join(checkout, file)));
  }

  await Bun.write(join(dest, 'LICENSE'), Bun.file(join(checkout, 'LICENSE')));
  await Bun.write(join(dest, 'REV'), `${rev}\n`);
  console.log(`.agent-tools/rune synced at ${rev}`);
} finally {
  await rm(checkout, { recursive: true, force: true });
}
