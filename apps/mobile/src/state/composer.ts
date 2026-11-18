// Each timeline's composer: what the message answers or edits, and the unsent text, kept per
// account and timeline so switching rooms keeps a draft (as the desktop's composers do).

import type { AccountId, Message, TimelineKey } from '@better-matrix/matrix-native/types';

import type { Store } from './store';
import { timelineId } from './timeline';
import type { State } from './types';

export interface ComposerState {
  /** The message the next send answers. */
  replyTo: Message | null;
  /** The user's own message being edited; the next send replaces its text. */
  editing: Message | null;
  /** The unsent text. */
  draft: string;
  /** Files waiting to go with the next send, in the order they were picked. */
  attachments?: PendingAttachment[];
  /** While editing: the draft, reply and attachments set aside, and the message's text as it read. */
  setAside?: SetAside | null;
  /** Where the caret was when the field last reported it: a picked emoji goes there. */
  selection?: { start: number; end: number };
  /** The user changed the draft this run: a kept draft read after that doesn't replace it. */
  touched?: boolean;
  /**
   * Edits the server refused while another edit held the field, newest first: each comes back
   * when the field is free (ADR 0006).
   */
  kept?: KeptEdit[];
}

/** An edit the server refused, kept to come back as the user left it. */
export interface KeptEdit {
  message: Message;
  /** The text the user sent. */
  text: string;
  /** The message's text as the edit first showed it. */
  original: string;
}

/** A file picked for the next send. */
export interface PendingAttachment {
  /** A plain path on disk, as `Upload::Path` takes it. */
  path: string;
  /** The name it is sent under (the path's last part). */
  name: string;
  /** How it previews in the composer. */
  kind: 'image' | 'video' | 'file';
  /** A `file://` URI for the preview. */
  uri: string;
}

/** What a composer held before an edit took the field (the desktop's `Draft`). */
export interface SetAside {
  draft: string;
  replyTo: Message | null;
  attachments: PendingAttachment[];
  /** The edited message's text as the field first showed it: an unchanged edit isn't sent. */
  original: string;
}

export const EMPTY_COMPOSER: ComposerState = { replyTo: null, editing: null, draft: '' };

export function composerId(account: AccountId, key: TimelineKey): string {
  return `${account}|${timelineId(key)}`;
}

export function composer(state: State, account: AccountId, key: TimelineKey): ComposerState {
  return state.composers[composerId(account, key)] ?? EMPTY_COMPOSER;
}

export function updateComposer(
  store: Store,
  account: AccountId,
  key: TimelineKey,
  change: (composer: ComposerState) => ComposerState,
): void {
  const id = composerId(account, key);
  store.update((s) => ({
    ...s,
    composers: { ...s.composers, [id]: change(s.composers[id] ?? EMPTY_COMPOSER) },
  }));
}

/** `composer` editing `edit`: what it held is set aside until the edit is sent or dropped. */
function editing(composer: ComposerState, edit: KeptEdit): ComposerState {
  const setAside: SetAside = {
    draft: composer.draft,
    replyTo: composer.replyTo,
    attachments: composer.attachments ?? [],
    original: edit.original,
  };

  return {
    ...composer,
    editing: edit.message,
    replyTo: null,
    draft: edit.text,
    attachments: [],
    setAside,
    selection: undefined,
  };
}

/** `composer` with any edit dropped and the draft set aside for it back; kept edits stay. */
function freed(composer: ComposerState): ComposerState {
  if (!composer.editing) return composer;
  const aside = composer.setAside;

  return {
    ...composer,
    editing: null,
    setAside: null,
    draft: aside?.draft ?? '',
    replyTo: aside?.replyTo ?? null,
    attachments: aside?.attachments ?? [],
    selection: undefined,
  };
}

/**
 * `composer` with any edit dropped: the draft set aside for it comes back, or the next kept
 * edit takes the field (with that draft set aside in turn).
 */
export function endEdit(composer: ComposerState): ComposerState {
  if (!composer.editing) return composer;
  const [next, ...rest] = composer.kept ?? [];

  return next ? editing({ ...freed(composer), kept: rest }, next) : freed(composer);
}

/**
 * The server refused `edit`: it comes back into the composer as the user left it, setting aside
 * whatever was typed since; while another edit holds the field it waits its turn; while the
 * same message is being edited again, that newer edit wins.
 */
export function refusedEdit(composer: ComposerState, edit: KeptEdit): ComposerState {
  if (!composer.editing) return editing(composer, edit);

  if (composer.editing.event_id === edit.message.event_id) return composer;

  const others = (composer.kept ?? []).filter(
    (kept) => kept.message.event_id !== edit.message.event_id,
  );

  return { ...composer, kept: [edit, ...others] };
}

/**
 * Answers `message` with the next send. Replying ends an edit: its draft comes back, and kept
 * edits wait for the next edit to end.
 */
export function startReply(
  store: Store,
  account: AccountId,
  key: TimelineKey,
  message: Message,
): void {
  updateComposer(store, account, key, (c) => ({ ...freed(c), replyTo: message }));
}

/**
 * Edits the user's own `message`: its text goes into the composer (`draftOf` in `format.ts`
 * writes a formatted message's HTML back as the composer's markers). The draft, reply and
 * attachments are set aside until the edit is sent or cancelled.
 */
export function startEdit(
  store: Store,
  account: AccountId,
  key: TimelineKey,
  message: Message,
  text: string,
): void {
  updateComposer(store, account, key, (c) => {
    // A kept edit of the same message comes back in its place; other kept edits wait.
    const kept = c.kept?.find((edit) => edit.message.event_id === message.event_id);
    const others = c.kept?.filter((edit) => edit !== kept);

    return editing({ ...freed(c), kept: others }, kept ?? { message, text, original: text });
  });
}
