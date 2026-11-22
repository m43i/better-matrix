// Drafts kept on this device (ADR 0007): what of a composer the engine's store keeps (the text as
// the user typed it, formatting markers and all, and the pills in it), and taking a kept draft
// back into a composer the user hasn't changed, so it never replaces what they did meanwhile.

import type { Draft } from '@better-matrix/matrix-native/types';

import { type ComposerState, EMPTY_COMPOSER } from './composer';
import { type EditDraft, editDraft } from './format';
import { type DraftPill, escapeHtml, livePills, pillAt, placePills } from './pills';

/** How often a draft that keeps changing is written to the store. */
export const KEEP_EVERY_MS = 500;

/**
 * What the store keeps of a composer: its text as typed (the draft set aside under an edit),
 * unless blank. Phones keep their own source with its markers as the body, so it comes back
 * exactly, spaces and escapes included; only phones read it back. A draft with pills keeps the
 * same source as `html` too, each pill standing as the desktop's field writes one
 * (`<span data-pill="id">label</span>`), the rest escaped text, so the pills come back as pills.
 */
export function storedDraft(composer: ComposerState | undefined): Draft | null {
  if (!composer) return null;
  const source = composer.editing ? composer.setAside : composer;
  const draft = source?.draft ?? '';

  if (draft.trim() === '') return null;
  const placed = placePills(draft, source?.pills ?? []);

  if (placed.length === 0) return { body: draft, html: null };
  let html = '';
  let at = 0;

  for (const { pill, start, end } of placed) {
    html += escapeHtml(draft.slice(at, start));
    html += `<span data-pill="${escapeHtml(pill.id)}">${escapeHtml(pill.label)}</span>`;
    at = end;
  }

  return { body: draft, html: html + escapeHtml(draft.slice(at)) };
}

/** Escaped text, or one pill as `storedDraft` writes it; nothing else. */
const SOURCE = /<span data-pill="([^"<>]*)">([^<]*)<\/span>|([^<]+)/y;

/** Text `storedDraft` escaped, as it was. */
function unescaped(text: string): string {
  return text.replace(/&(amp|lt|gt|quot);/g, (_, name: string) =>
    name === 'amp' ? '&' : name === 'lt' ? '<' : name === 'gt' ? '>' : '"',
  );
}

/**
 * A draft `storedDraft` kept with its pills: `null` unless `html` is that source and spells
 * `body` exactly (else it is Matrix HTML, read as an edit's).
 */
function keptSource(body: string, html: string): EditDraft | null {
  const pills: DraftPill[] = [];
  let draft = '';
  SOURCE.lastIndex = 0;

  while (SOURCE.lastIndex < html.length) {
    const match = SOURCE.exec(html);

    if (!match) return null;
    const [, id, label, text] = match;

    if (id !== undefined && label !== undefined) {
      const words = unescaped(label);
      pills.push(pillAt({ id: unescaped(id), label: words }, draft.length));
      draft += words;
    } else {
      draft += unescaped(text ?? '');
    }
  }

  return draft === body ? { draft, pills: livePills(draft, pills) } : null;
}

/** A kept draft as the composer holds it: its text and pills. */
function keptDraft({ body, html }: Draft): EditDraft {
  return (html !== null && keptSource(body, html)) || editDraft(body, html);
}

export function sameDraft(a: Draft | null, b: Draft | null): boolean {
  return a?.body === b?.body && (a?.html ?? null) === (b?.html ?? null);
}

/**
 * A kept draft back in `composer` if the user hasn't changed its draft and its text is blank:
 * into the field (beside a reply or files picked meanwhile), or under an edit started meanwhile,
 * as the draft it sets aside. Its pills come back with it. Else the composer as it is.
 */
export function withKeptDraft(composer: ComposerState | undefined, draft: Draft): ComposerState {
  const current = composer ?? EMPTY_COMPOSER;
  const aside = current.setAside;

  if (current.touched) return current;

  if (current.editing) {
    if (!aside || aside.draft.trim() !== '') return current;
    const kept = keptDraft(draft);

    return { ...current, setAside: { ...aside, draft: kept.draft, pills: kept.pills } };
  }

  if (current.draft.trim() !== '') return current;
  const kept = keptDraft(draft);

  return { ...current, draft: kept.draft, pills: kept.pills };
}
