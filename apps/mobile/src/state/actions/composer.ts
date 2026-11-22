// What the composer, the emoji picker and the poll form do: drafts, attachments, sending,
// edits, typing notices, reactions and polls. Each send's failure becomes the desktop's notice.

import type { AccountId, Command, TimelineKey } from '@better-matrix/matrix-native/types';

import { type Key, t } from '../../i18n';
import {
  composer,
  endEdit,
  type PendingAttachment,
  refusedEdit,
  updateComposer,
} from '../composer';
import { rememberEmoji } from '../emoji';
import {
  canSend,
  insertAt,
  planSchedule,
  planSend,
  type ScheduleProblem,
  scheduleProblem,
  type SendPlan,
  type SendProblem,
  voiceMessage,
  withAttachments,
} from '../outgoing';
import {
  applySplices,
  type DraftPill,
  editPills,
  livePills,
  type Pill,
  pillAt,
  type Splice,
  splicePills,
  trackPills,
} from '../pills';
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
): Promise<boolean> {
  return store.run(command).then(
    () => true,
    (cause: unknown) => {
      if (cause instanceof CommandFailed && cause.cancelled) return false;
      failed?.();
      const message = cause instanceof Error ? cause.message : String(cause);
      store.update((s) => addNotice(s, t(key, { error: message })));

      return false;
    },
  );
}

/**
 * The draft changed (and where the caret is). Its pills move with the edit; one whose label was
 * edited away is gone. `caret`, where the field's caret was, tells apart edits in repeated text.
 */
export function setDraft(
  store: Store,
  account: AccountId,
  key: TimelineKey,
  draft: string,
  selection?: { start: number; end: number },
  caret?: number,
): void {
  updateComposer(store, account, key, (c) => ({
    ...c,
    draft,
    pills:
      c.pills && c.pills.length > 0
        ? trackPills(c.draft, draft, c.pills, caret ?? selection?.start)
        : c.pills,
    selection: selection ?? c.selection,
    // Typing in an edit changes the sent message, not the draft set aside under it.
    touched: c.touched || !c.editing,
  }));
}

/**
 * The draft changed by the app at known places (the formatting toolbar, a list continued):
 * `splices` are the changes made to the draft as the store has it, and its pills move by them
 * exactly, so a marker put around a pill keeps it. Were the draft no longer what the splices
 * were made from, the change is measured as the user's would be.
 */
export function spliceDraft(
  store: Store,
  account: AccountId,
  key: TimelineKey,
  edited: { text: string; selection: { start: number; end: number }; splices: Splice[] },
): void {
  updateComposer(store, account, key, (c) => {
    const pills = c.pills ?? [];

    const moved =
      applySplices(c.draft, edited.splices) === edited.text
        ? livePills(edited.text, splicePills(pills, edited.splices))
        : trackPills(c.draft, edited.text, pills, edited.selection.start);

    return {
      ...c,
      draft: edited.text,
      pills: pills.length > 0 ? moved : c.pills,
      selection: edited.selection,
      touched: c.touched || !c.editing,
    };
  });
}

/**
 * The draft with a suggestion picked: `draft` (the trigger's text replaced) and the pill it
 * inserted, if it was one (a mention, a custom emoji), where it was put: the pill stands for
 * that place, whatever the same words say elsewhere.
 */
export function completeDraft(
  store: Store,
  account: AccountId,
  key: TimelineKey,
  draft: string,
  selection: { start: number; end: number },
  pill: DraftPill | null,
): void {
  updateComposer(store, account, key, (c) => {
    const moved = trackPills(c.draft, draft, c.pills ?? [], pill?.start);
    const pills = pill ? [...moved, pill] : moved;

    return {
      ...c,
      draft,
      pills: livePills(draft, pills),
      selection,
      touched: c.touched || !c.editing,
    };
  });
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

/**
 * Puts `text` at the caret of a timeline's draft (an emoji picked for it); `pill` when it is
 * one (a custom emoji, its short code the text).
 */
export function insertIntoDraft(
  store: Store,
  account: AccountId,
  key: TimelineKey,
  text: string,
  pill: Pill | null = null,
): void {
  updateComposer(store, account, key, (c) => {
    const inserted = insertAt(c.draft, c.selection, text);
    const start = inserted.selection.start - text.length;
    const end = start + c.draft.length - (inserted.text.length - text.length);
    const moved = editPills(c.pills ?? [], start, end, text);
    const pills = pill ? [...moved, pillAt(pill, start)] : moved;

    return {
      ...c,
      draft: inserted.text,
      pills: livePills(inserted.text, pills),
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

/** Where the app goes after a command from the composer worked. */
export interface AfterCommand {
  /** A room was joined (`/join`): open it. */
  joined?: (roomId: string) => void;
  /** The room was left (`/leave`): leave its screen. */
  left?: () => void;
}

/** What a command that can't be done says, as a notice. */
function problemText(problem: SendProblem): string {
  return problem.type === 'unknown'
    ? t('mobile.composer.command_unknown', { name: problem.name })
    : t('mobile.composer.command_usage', { name: problem.name, args: t(problem.args) });
}

/**
 * Sends what the composer holds (or saves the edit under way) and empties it. Returns whether
 * anything was sent. A command that can't be done leaves the draft as it was and says why.
 */
export function sendComposer(
  store: Store,
  account: AccountId,
  key: TimelineKey,
  after: AfterCommand = {},
): boolean {
  const current = composer(store.getState(), account, key);

  if (!canSend(current) && !current.editing) return false;
  const plan = planSend(current, account, key);

  if (plan.problem) {
    const problem = plan.problem;
    store.update((s) => addNotice(s, problemText(problem), account));

    return false;
  }

  if (plan.after) {
    carryOut(store, account, key, current, plan, after);

    return true;
  }

  // A draft sent is a draft the user changed: a kept one read late doesn't come back over it.
  updateComposer(store, account, key, () => ({ ...plan.composer, touched: current.touched }));

  // A refused edit comes back as the user left it (ADR 0006), unless a newer edit of the
  // message was sent since: that one's outcome decides.
  const edit = current.editing && {
    message: current.editing,
    text: current.draft,
    pills: current.pills ?? [],
    original: current.setAside?.original ?? '',
    originalPills: current.setAside?.originalPills ?? [],
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

/** Sends a command from the composer, going where it says once it worked. */
function carryOut(
  store: Store,
  account: AccountId,
  key: TimelineKey,
  before: ReturnType<typeof composer>,
  plan: SendPlan,
  after: AfterCommand,
): void {
  updateComposer(store, account, key, () => ({ ...plan.composer, touched: before.touched }));

  for (const command of plan.commands) {
    let joined: string | null = null;
    store
      .run(command, (event) => {
        if (event.type === 'RoomJoined') joined = event.data.room_id;
      })
      .then(
        () => {
          if (plan.after === 'joined') after.joined?.(joined ?? '');

          if (plan.after === 'left') after.left?.();
        },
        (cause: unknown) => {
          if (cause instanceof CommandFailed && cause.cancelled) return;
          const message = cause instanceof Error ? cause.message : String(cause);
          store.update((s) => addNotice(s, t(failedKey(command), { error: message }), account));
        },
      );
  }
}

/** What a command from the composer says when it fails. */
function failedKey(command: Command): Key {
  switch (command.type) {
    case 'Invite':
      return 'notice.invite';
    case 'JoinRoom':
      return 'notice.join';
    case 'LeaveRoom':
      return 'notice.leave';
    default:
      return 'mobile.composer.command_failed';
  }
}

/** What stops a message from being scheduled, as the user is told; `null` when nothing need be. */
export function scheduleProblemText(problem: ScheduleProblem): string | null {
  switch (problem.type) {
    case 'attachments':
      return t('mobile.composer.schedule_attachments');
    case 'command':
      return t('mobile.composer.schedule_command');
    case 'nothing':
      return null;
    default:
      return problemText(problem);
  }
}

/**
 * Asks the server to send what the composer holds in `delayMs` (MSC4140) and empties it: text,
 * or a command that makes a message. Anything else (a command that acts, attachments) isn't
 * done: the draft stays and a notice says why. Returns whether it was scheduled.
 */
export function scheduleComposer(
  store: Store,
  account: AccountId,
  key: TimelineKey,
  delayMs: number,
): boolean {
  const current = composer(store.getState(), account, key);
  const problem = scheduleProblem(current, account, key);
  const plan = problem ? null : planSchedule(current, account, key, delayMs);

  if (!plan) {
    const text = problem && scheduleProblemText(problem);

    if (text) store.update((s) => addNotice(s, text, account));

    return false;
  }

  updateComposer(store, account, key, () => ({ ...plan.composer, touched: current.touched }));

  for (const command of plan.commands)
    void sendTold(store, command, 'notice.not_sent', () =>
      // A refused schedule gives the composer back as it was (its text, pills and the message
      // it answered); were something written, answered or edited since, once that is done.
      updateComposer(store, account, key, (c) => ({
        ...c,
        refused: [
          ...(c.refused ?? []),
          { draft: current.draft, pills: current.pills ?? [], replyTo: current.replyTo },
        ],
      })),
    );

  return true;
}

/** Sends a recorded voice memo; its reply target, if any, stays for the next text. */
export function sendVoiceMemo(
  store: Store,
  account: AccountId,
  key: TimelineKey,
  path: string,
  durationMs: number,
  waveform: number[] = [],
): void {
  sendTold(
    store,
    {
      type: 'Send',
      data: { account, key, reply_to: null, message: voiceMessage(path, durationMs, waveform) },
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
