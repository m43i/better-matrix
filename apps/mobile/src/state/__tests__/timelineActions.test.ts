import { detachedEngine } from '@better-matrix/matrix-native/client';
import type { Poll } from '@better-matrix/matrix-native/types';

import { joinCall } from '../actions/calls';
import {
  deleteMessage,
  endPoll,
  fetchOriginal,
  react,
  releaseTimeline,
  retainTimeline,
  retryHistory,
  vote,
} from '../actions/timeline';
import { timeline as timelineOf } from '../selectors';
import { Store } from '../store';
import { roomKey } from '../timeline';
import { ADA, summary } from '../../test/fixtures';

const key = roomKey('!r');

function setup() {
  const { engine, sent } = detachedEngine();
  const store = new Store(engine);
  engine.emit({ type: 'AccountAdded', data: summary(ADA) });

  return { engine, store, types: () => sent.map((s) => s.command.type), sent };
}

const poll: Poll = {
  question: '?',
  answers: [
    { id: 'a', text: 'a', votes: 1, mine: true },
    { id: 'b', text: 'b', votes: 0, mine: false },
  ],
  disclosed: true,
  max_selections: 1,
  voters: 1,
  ended: false,
};

describe('following a timeline', () => {
  it('should open a timeline for the first screen showing it', () => {
    const { store, types } = setup();
    retainTimeline(store, ADA, key);
    retainTimeline(store, ADA, key);
    expect(types()).toEqual(['OpenTimeline']);
  });

  it('should keep a timeline open while another screen shows it', () => {
    const { store, types } = setup();
    retainTimeline(store, ADA, key);
    retainTimeline(store, ADA, key);
    releaseTimeline(store, ADA, key);
    expect(types()).toEqual(['OpenTimeline']);
  });

  it('should close a timeline when the last screen stops showing it', () => {
    const { store, types } = setup();
    retainTimeline(store, ADA, key);
    releaseTimeline(store, ADA, key);
    expect(types()).toEqual(['OpenTimeline', 'CloseTimeline']);
  });
});

describe('timeline actions', () => {
  it('should react through the timeline', () => {
    const { store, sent } = setup();
    react(store, ADA, key, '$m', '👍');
    expect(sent[0]?.command).toEqual({
      type: 'React',
      data: { account: ADA, key, event_id: '$m', emoji: '👍' },
    });
  });

  it('should vote the poll next vote', () => {
    const { store, sent } = setup();
    vote(store, ADA, key, '$p', poll, 'b');
    expect(sent[0]?.command).toEqual({
      type: 'VotePoll',
      data: { account: ADA, key, poll_id: '$p', answers: ['b'] },
    });
  });

  it('should not vote in an ended poll', () => {
    const { store, sent } = setup();
    vote(store, ADA, key, '$p', { ...poll, ended: true }, 'b');
    expect(sent).toEqual([]);
  });

  it('should end a poll', () => {
    const { store, sent } = setup();
    endPoll(store, ADA, key, '$p');
    expect(sent[0]?.command).toEqual({
      type: 'EndPoll',
      data: { account: ADA, key, poll_id: '$p' },
    });
  });

  it('should delete a message by redacting it', () => {
    const { store, sent } = setup();
    deleteMessage(store, ADA, key, '$m');
    expect(sent[0]?.command).toEqual({
      type: 'Redact',
      data: { account: ADA, key, event_id: '$m' },
    });
  });

  it('should ask again for a page that failed', () => {
    const { engine, store, types } = setup();
    engine.emit({ type: 'HistoryFailed', data: { account: ADA, key, error: 'offline' } });
    retryHistory(store, ADA, key);
    expect(types()).toEqual(['LoadOlder']);
  });

  it('a timeline that failed to open should show why and open again on retry', async () => {
    const { engine, store, types, sent } = setup();
    retainTimeline(store, ADA, key);
    engine.emit({
      type: 'Finished',
      data: { request: sent[0]!.request, result: { Err: 'store locked' } },
    });
    await Promise.resolve();
    expect(timelineOf(store.getState(), ADA, key).historyError).toBe('store locked');
    retryHistory(store, ADA, key);
    expect(types()).toEqual(['OpenTimeline', 'OpenTimeline']);
    expect(timelineOf(store.getState(), ADA, key).historyError).toBeNull();
  });

  it('should join a call by starting it', () => {
    const { store, sent } = setup();
    joinCall(store, ADA, '!r', true);
    expect(sent[0]?.command).toEqual({
      type: 'StartCall',
      data: { account: ADA, room_id: '!r', video: true },
    });
  });
});

describe('fetchOriginal', () => {
  it('should give the file of media already fetched without asking again', async () => {
    const { engine, store, sent } = setup();
    engine.emit({
      type: 'MediaFile',
      data: { request: 9, media: 'mxc-a', size: { type: 'Original' }, path: '/c/a.png' },
    });
    await expect(fetchOriginal(store, ADA, 'mxc-a')).resolves.toBe('file:///c/a.png');
    expect(sent).toEqual([]);
  });

  it('should fetch the original and give its file', async () => {
    const { engine, store, sent } = setup();
    const done = fetchOriginal(store, ADA, 'mxc-a');
    const request = sent[0]?.request ?? 0;
    engine.emit({
      type: 'MediaFile',
      data: { request, media: 'mxc-a', size: { type: 'Original' }, path: '/c/a.png' },
    });
    engine.emit({ type: 'Finished', data: { request, result: { Ok: null } } });
    await expect(done).resolves.toBe('file:///c/a.png');
  });
});
