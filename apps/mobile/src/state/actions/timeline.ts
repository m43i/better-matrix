// What the user does in a timeline: follow it while a screen shows it, react, vote, end a poll,
// delete a message, retry a failed page, and fetch an attachment's file.

import type { AccountId, MediaRef, Poll, TimelineKey } from '@better-matrix/matrix-native/types';

import { nextVote } from '../messages';
import { mediaUri, timeline as timelineOf } from '../selectors';
import type { Store } from '../store';
import { timelineId } from '../timeline';
import { closeTimeline, loadOlder, openTimeline } from './core';

/** How many screens show each timeline, per store. */
const shown = new WeakMap<Store, Map<string, number>>();

/**
 * A screen starts showing a timeline: the first one opens it. A timeline stays open while any
 * screen shows it, and while sheets over that screen (message actions, the emoji picker) act on
 * it, since the engine reacts, edits and deletes only through open timelines.
 */
export function retainTimeline(store: Store, account: AccountId, key: TimelineKey): void {
  const counts = shown.get(store) ?? new Map<string, number>();
  shown.set(store, counts);
  const id = `${account}|${timelineId(key)}`;
  const count = counts.get(id) ?? 0;
  counts.set(id, count + 1);
  if (count === 0) openTimeline(store, account, key);
}

/**
 * Resolves once the timeline isn't opening any more: true when it is open, false when it didn't
 * open.
 */
export function untilOpened(store: Store, account: AccountId, key: TimelineKey): Promise<boolean> {
  return new Promise((resolve) => {
    const settled = () => {
      const now = timelineOf(store.getState(), account, key);
      if (now.opening) return false;
      resolve(!now.openFailed);
      return true;
    };
    if (settled()) return;
    const stop = store.subscribe(() => {
      if (settled()) stop();
    });
  });
}

/** A screen stops showing a timeline: the last one closes it. */
export function releaseTimeline(store: Store, account: AccountId, key: TimelineKey): void {
  const counts = shown.get(store);
  const id = `${account}|${timelineId(key)}`;
  const count = counts?.get(id) ?? 0;
  if (count <= 0) return;
  if (count === 1) {
    counts?.delete(id);
    closeTimeline(store, account, key);
  } else {
    counts?.set(id, count - 1);
  }
}

/** Adds the user's reaction, or takes it back. */
export function react(store: Store, account: AccountId, key: TimelineKey, eventId: string, emoji: string): void {
  store.send({ type: 'React', data: { account, key, event_id: eventId, emoji } });
}

/** Votes for `answer` in a poll, as pressing it does (see `nextVote`). Ended polls take no votes. */
export function vote(store: Store, account: AccountId, key: TimelineKey, pollId: string, poll: Poll, answer: string): void {
  if (poll.ended) return;
  store.send({ type: 'VotePoll', data: { account, key, poll_id: pollId, answers: nextVote(poll, answer) } });
}

/** Ends a poll the user started; nobody can vote after. */
export function endPoll(store: Store, account: AccountId, key: TimelineKey, pollId: string): void {
  store.send({ type: 'EndPoll', data: { account, key, poll_id: pollId } });
}

/** Deletes one of the user's messages, for everyone. */
export function deleteMessage(store: Store, account: AccountId, key: TimelineKey, eventId: string): void {
  store.send({ type: 'Redact', data: { account, key, event_id: eventId } });
}

/** Asks again for the page of history that failed, or opens again a timeline that didn't open. */
export function retryHistory(store: Store, account: AccountId, key: TimelineKey): void {
  if (timelineOf(store.getState(), account, key).openFailed) openTimeline(store, account, key);
  else loadOlder(store, account, key);
}

/** The `file://` URI of an attachment's original file, fetched first if it isn't yet. */
export async function fetchOriginal(store: Store, account: AccountId, media: MediaRef): Promise<string> {
  const original = { type: 'Original' } as const;
  const known = mediaUri(store.getState(), media, original);
  if (known) return known;
  await store.run({ type: 'FetchMedia', data: { account, media, size: original } });
  const uri = mediaUri(store.getState(), media, original);
  if (!uri) throw new Error('media not loaded');
  return uri;
}
