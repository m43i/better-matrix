// What the composer, the emoji picker and the poll form do: drafts, attachments, sending,
// edits, typing notices, reactions and polls. Each send's failure becomes the desktop's notice.

import type { AccountId, Command, TimelineKey } from '@better-matrix/matrix-native/types';

import { t } from '../../i18n';
import {
  composer,
  endEdit,
  type PendingAttachment,
  refusedEdit,
  updateComposer,
} from '../composer';
import { rememberEmoji } from '../emoji';
import { canSend, insertAt, planSend, voiceMessage, withAttachments } from '../outgoing';
import { type PollForm, pollOutgoing } from '../pollForm';
import { addNotice } from '../reducer';
import { CommandFailed } from '../requests';
import type { Store } from '../store';
import { roomKey, threadKey } from '../timeline';

/** The timeline a route's `roomId` and optional `thread` params name. */
export function keyOf(roomId: string, thread?: string | null): TimelineKey {
  return thread ? threadKey(roomId, thread) : roomKey(roomId);
}

/**
 * Sends `command`; a failure shows as the notice `key` says ("Message not sent: …"), after
 * `failed` had its say.
 */
function sendTold(
  store: Store,
  command: Command,
  key: 'notice.not_sent' | 'notice.not_changed' | 'notice.react',
  failed?: () => void,
): void {
  store.run(command).catch((cause: unknown) => {
    if (cause instanceof CommandFailed && cause.cancelled) return;
    failed?.();
    const message = cause instanceof Error ? cause.message : String(cause);
    store.update((s) => addNotice(s, t(key, { error: message })));
  });
}

/** The draft changed (and where the caret is). */
export function setDraft(
  store: Store,
  account: AccountId,
  key: TimelineKey,
  draft: string,
  selection?: { start: number; end: number },
): void {
  updateComposer(store, account, key, (c) => ({
    ...c,
    draft,
    selection: selection ?? c.selection,
    // Typing in an edit changes the sent message, not the draft set aside under it.
    touched: c.touched || !c.editing,
  }));
}

/** Remembers where the caret is, for an emoji picked later. */
export function setSelection(
  store: Store,
  account: AccountId,
  key: TimelineKey,
  selection: { start: number; end: number },
): void {
  updateComposer(store, account, key, (c) => ({ ...c, selection }));
}

/** Puts `text` at the caret of a timeline's draft (an emoji picked for it). */
export function insertIntoDraft(
  store: Store,
  account: AccountId,
  key: TimelineKey,
  text: string,
): void {
  updateComposer(store, account, key, (c) => {
    const inserted = insertAt(c.draft, c.selection, text);

    return {
      ...c,
      draft: inserted.text,
      selection: inserted.selection,
      touched: c.touched || !c.editing,
    };
  });
}

export function cancelReply(store: Store, account: AccountId, key: TimelineKey): void {
  updateComposer(store, account, key, (c) => ({ ...c, replyTo: null }));
}

/** Stops editing: the draft set aside comes back. */
export function cancelEdit(store: Store, account: AccountId, key: TimelineKey): void {
  updateComposer(store, account, key, endEdit);
}

/** Adds picked files to the next send, each once. */
export function addAttachments(
  store: Store,
  account: AccountId,
  key: TimelineKey,
  picked: readonly PendingAttachment[],
): void {
  if (picked.length === 0) return;
  updateComposer(store, account, key, (c) => withAttachments(c, picked));
}

export function removeAttachment(
  store: Store,
  account: AccountId,
  key: TimelineKey,
  path: string,
): void {
  updateComposer(store, account, key, (c) => ({
    ...c,
    attachments: (c.attachments ?? []).filter((a) => a.path !== path),
  }));
}

/** The newest edit sent of each message, per store: a refusal of an older one is stale. */
const newestEdits = new WeakMap<Store, Map<string, number>>();

let editsSent = 0;

/**
 * Sends what the composer holds (or saves the edit under way) and empties it. Returns whether
 * anything was sent.
 */
export function sendComposer(store: Store, account: AccountId, key: TimelineKey): boolean {
  const current = composer(store.getState(), account, key);

  if (!canSend(current) && !current.editing) return false;
  const plan = planSend(current, account, key);
  // A draft sent is a draft the user changed: a kept one read late doesn't come back over it.
  updateComposer(store, account, key, () => ({ ...plan.composer, touched: current.touched }));

  // A refused edit comes back as the user left it (ADR 0006), unless a newer edit of the
  // message was sent since: that one's outcome decides.
  const edit = current.editing && {
    message: current.editing,
    text: current.draft,
    original: current.setAside?.original ?? '',
  };

  const newest = newestEdits.get(store) ?? new Map<string, number>();
  newestEdits.set(store, newest);
  const editKey = edit && `${account}|${edit.message.event_id}`;
  const sent = (editsSent += 1);

  if (editKey) newest.set(editKey, sent);

  const refused = () => {
    if (!edit || !editKey || newest.get(editKey) !== sent) return;
    updateComposer(store, account, key, (c) => refusedEdit(c, edit));
  };

  for (const command of plan.commands) {
    if (command.type === 'Edit') sendTold(store, command, 'notice.not_changed', refused);
    else sendTold(store, command, 'notice.not_sent');
  }

  return plan.commands.length > 0;
}

/** Sends a recorded voice memo; its reply target, if any, stays for the next text. */
export function sendVoiceMemo(
  store: Store,
  account: AccountId,
  key: TimelineKey,
  path: string,
  durationMs: number,
): void {
  sendTold(
    store,
    {
      type: 'Send',
      data: { account, key, reply_to: null, message: voiceMessage(path, durationMs) },
    },
    'notice.not_sent',
  );
}

/** Sends a poll once the form is ready; returns whether it went. */
export function sendPoll(
  store: Store,
  account: AccountId,
  key: TimelineKey,
  form: PollForm,
): boolean {
  const message = pollOutgoing(form);

  if (!message) return false;
  sendTold(
    store,
    { type: 'Send', data: { account, key, reply_to: null, message } },
    'notice.not_sent',
  );

  return true;
}

/** Reacts to a message with `emoji` (or takes the user's reaction back), and remembers it. */
export function react(
  store: Store,
  account: AccountId,
  key: TimelineKey,
  eventId: string,
  emoji: string,
): void {
  noteEmoji(store, emoji);
  sendTold(
    store,
    { type: 'React', data: { account, key, event_id: eventId, emoji } },
    'notice.react',
  );
}

/** Puts `emoji` first among the recently used. */
export function noteEmoji(store: Store, emoji: string): void {
  store.update((s) => ({ ...s, recentEmoji: rememberEmoji(s.recentEmoji, emoji) }));
}

/** Tells a room whether the user is typing in it. */
export function sendTyping(
  store: Store,
  account: AccountId,
  roomId: string,
  typing: boolean,
): void {
  store.send({ type: 'Typing', data: { account, room_id: roomId, typing } }, { quiet: true });
}
