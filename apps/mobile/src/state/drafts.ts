// Drafts kept on this device (ADR 0007): what of a composer the engine's store keeps (the text as
// the user typed it, formatting markers and all), and taking a kept draft back into a composer the
// user hasn't changed, so it never replaces what they did meanwhile.

import type { Draft } from '@better-matrix/matrix-native/types';

import { type ComposerState, EMPTY_COMPOSER } from './composer';
import { draftOf } from './format';

/** How often a draft that keeps changing is written to the store. */
export const KEEP_EVERY_MS = 500;

/**
 * What the store keeps of a composer: its text as typed (the draft set aside under an edit),
 * unless blank. Phones keep their own source with its markers as the body, so it comes back
 * exactly, spaces and escapes included; only phones read it back.
 */
export function storedDraft(composer: ComposerState | undefined): Draft | null {
  if (!composer) return null;
  const draft = composer.editing ? (composer.setAside?.draft ?? '') : composer.draft;

  return draft.trim() === '' ? null : { body: draft, html: null };
}

export function sameDraft(a: Draft | null, b: Draft | null): boolean {
  return a?.body === b?.body && (a?.html ?? null) === (b?.html ?? null);
}

/** Nothing in it at all: no text, reply, edit or attachment. */
export function isBlank(composer: ComposerState): boolean {
  return (
    composer.draft.trim() === '' &&
    !composer.replyTo &&
    !composer.editing &&
    (composer.attachments ?? []).length === 0
  );
}

/**
 * A kept draft back in `composer` if the user hasn't changed its draft: into the field, or under
 * an edit started meanwhile, as the draft it sets aside. Else the composer as it is.
 */
export function withKeptDraft(composer: ComposerState | undefined, draft: Draft): ComposerState {
  const current = composer ?? EMPTY_COMPOSER;
  const text = draftOf(draft.body, draft.html);
  const aside = current.setAside;

  if (current.touched) return current;

  if (isBlank(current)) return { ...current, draft: text };

  if (
    current.editing &&
    aside &&
    aside.draft.trim() === '' &&
    !aside.replyTo &&
    aside.attachments.length === 0
  ) {
    return { ...current, setAside: { ...aside, draft: text } };
  }

  return current;
}
