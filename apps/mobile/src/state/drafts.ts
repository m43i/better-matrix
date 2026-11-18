// Drafts kept on this device (ADR 0007): what of a composer the engine's store keeps (its text
// and formatting, as the desktop sends them), and taking a kept draft back into a composer that
// holds nothing, so it never replaces what the user did meanwhile.

import type { Draft } from '@better-matrix/matrix-native/types';

import { type ComposerState, EMPTY_COMPOSER } from './composer';
import { draftOf, outgoingText } from './format';

/** How often a draft that keeps changing is written to the store. */
export const KEEP_EVERY_MS = 500;

/** What the store keeps of a composer: its text (the draft set aside under an edit), unless blank. */
export function storedDraft(composer: ComposerState | undefined): Draft | null {
  if (!composer) return null;
  const draft = composer.editing ? (composer.setAside?.draft ?? '') : composer.draft;
  const { body, html } = outgoingText(draft);

  return body === '' ? null : { body, html };
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

/** A kept draft back in `composer`, if it holds nothing; else the composer as it is. */
export function withKeptDraft(composer: ComposerState | undefined, draft: Draft): ComposerState {
  const current = composer ?? EMPTY_COMPOSER;

  return isBlank(current) ? { ...current, draft: draftOf(draft.body, draft.html) } : current;
}
