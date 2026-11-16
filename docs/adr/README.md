# Architecture decision records

One record per area. Each states the decisions in force there, why they were made, and what
they cost. When a decision changes, update its area's record in place; a new area gets a new
record and a line here.

| ADR                                      | Area                                                                                    |
| ---------------------------------------- | --------------------------------------------------------------------------------------- |
| [0001](0001-stack-and-tooling.md)        | Stack and tooling: monorepo, Rune, matrix-rust-sdk, builds, translations, fonts         |
| [0002](0002-engine-and-desktop-state.md) | Engine and desktop state: the actor, the reducer, purposes, overlays, the shell         |
| [0003](0003-accounts-and-sign-in.md)     | Accounts and sign-in: sessions, secrets, OAuth, restoring, ended sessions, the backdrop |
| [0004](0004-encryption.md)               | Encryption: recovery, verification, sessions, identity reset, room keys                 |
| [0005](0005-navigation-and-settings.md)  | Navigation and settings: rail, scopes, sections, notifications, interface size          |
| [0006](0006-timelines.md)                | Timelines: history, threads, reactions, room events, GIFs, typing, read receipts        |
| [0007](0007-composer-and-media.md)       | Composer and media: rich text, uploads, voice, previews, image cache, retries           |
| [0008](0008-people-and-rooms.md)         | People and rooms: profile, contact details, search, members, making rooms               |
| [0009](0009-calls.md)                    | Calls: MatrixRTC and one-to-one calls, media engine, ringing, the call stage            |
| [0010](0010-mobile.md)                   | Mobile: React Native and Expo, the bridge, JSON boundary, the store, OAuth on phones    |

## Template

```markdown
# NNNN. Area

Status: Accepted
Date: YYYY-MM-DD

## Context

What the area is and the forces that shape it.

## Decision

What we do, grouped by topic.

## Consequences

What gets easier, what gets harder, what to watch.
```
