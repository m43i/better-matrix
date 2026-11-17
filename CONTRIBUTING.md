# Contributing to Better Matrix

Bug reports, feature ideas, documentation, translations, tests, and code are welcome. Please
follow our [Code of Conduct](CODE_OF_CONDUCT.md) in all project spaces.

## Before you start

Search existing issues and pull requests to avoid duplicating work. For a substantial feature
or architectural change, open an issue first to discuss the problem and proposed approach.
Small fixes and documentation improvements can go straight to a pull request.

For bug reports, include your platform, app version, steps to reproduce, expected behaviour,
and what actually happened. Share relevant logs or screenshots, but remove access tokens,
private messages, room names, and other sensitive information.

For feature requests, explain the use case and the behaviour you would like to see.

## Development setup

Fork the repository if you do not have write access, clone it, and create a branch from `main`.
Follow the [README's development instructions](README.md#develop) for prerequisites, installation,
and commands to run the desktop or mobile app.

Read [AGENTS.md](AGENTS.md) for repository conventions, the [glossary](docs/glossary.md) for shared
terms, and the relevant [architecture decision record](docs/adr/README.md) before changing an area.

Keep changes focused. Follow the surrounding code's style, add or update tests for changed
behaviour, and update documentation when needed. Every new user-facing string belongs in the
English, German, and French catalogs. Update the area's ADR when a decision changes, and add new
project terms to the glossary.

Before submitting code, run from the repository root:

```sh
bun run lint
bun run test
```

Use `bun run lint:fix` and `bun run format` when fixes or formatting are needed. For UI changes,
also check the relevant desktop headless scene or mobile demo scene and include screenshots.
See [AGENTS.md](AGENTS.md) for the platform-specific checks.

## Pull requests

Open pull requests against `main` and fill out the pull request template. Describe what changed
and why, link related issues (for example, `Fixes #123`), and explain how you checked the change.
Use a draft pull request when you want early feedback or the work is still in progress.

### Titles

Every pull request title must use this format:

```text
type(module): short description
```

Use a lowercase type and a module that identifies the affected area, such as `desktop`, `mobile`,
`matrix-core`, or `docs`. Common types are:

| Type       | Use for                                        |
| ---------- | ---------------------------------------------- |
| `fix`      | Bug fixes                                      |
| `feat`     | New features                                   |
| `perf`     | Performance improvements                       |
| `refactor` | Code restructuring without changing behaviour  |
| `docs`     | Documentation                                  |
| `test`     | Adding or improving tests                      |
| `build`    | Build tooling or packaging                     |
| `ci`       | Continuous integration and release workflows   |
| `chore`    | Other maintenance, including dependency updates |
| `style`    | Formatting changes without changing behaviour  |
| `revert`   | Reverting an earlier change                     |

Examples:

```text
fix(matrix-core): restore saved sessions after restart
feat(desktop): add room search
perf(mobile): reduce timeline rendering work
refactor(matrix-core): simplify command dispatch
docs(community): add contribution guidelines
```

### Labels

Every pull request must have the labels that fit its changes before it is merged. Labels drive
the release notes, so a correctly formatted title alone is not enough.

Apply at least one change label from [.github/labels.yml](.github/labels.yml):

- `feature` for new capabilities, `enhancement` for improvements, `bug` for fixes, or `performance`
  for performance work.
- `security`, `accessibility`, or `i18n` when the change affects those areas.
- `documentation` for docs, `dependencies` for dependency updates, or `ci` for build, CI,
  packaging, and release tooling.
- `skip-changelog` for internal changes that should be left out of the release notes, such as
  a refactor or test-only change with no user-visible effect.

Add `breaking` when the change breaks something users or packagers rely on, and add any
applicable platform label (`platform: linux`, `platform: macos`, or `platform: windows`). Use
multiple labels when appropriate. If you cannot apply labels yourself, say which ones fit in
the pull request description so a maintainer can attach them before merging.

Respond to review feedback and keep the pull request focused until it is ready to merge.

## License

By contributing, you agree that your contributions will be distributed under the project's
[MIT license](LICENSE). Only submit work you have the right to contribute.
