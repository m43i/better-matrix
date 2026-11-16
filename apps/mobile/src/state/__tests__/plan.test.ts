import type { TimelineEntry } from '@better-matrix/matrix-native/types';

import { atStart, GROUP_WINDOW_MS, newestSent, planTimeline, rowOf, shouldMarkRead } from '../plan';
import {
  ADA,
  entry,
  GRACE,
  message,
  messageEntry,
  person,
  roomEventEntry,
  timelineOf,
  withContent,
} from '../../test/fixtures';

const room = { thread: null };

const kinds = (entries: TimelineEntry[], change = {}) =>
  planTimeline(timelineOf(entries, change), room).map((r) => r.kind);

const ada = person(ADA, 'Ada');

const grace = person(GRACE, 'Grace');

const T = 1_700_000_000_000;

const marked = (eventId: string) => ({ eventId, fullyRead: false });

describe('planTimeline', () => {
  it('should give each message a row', () => {
    expect(kinds([entry('a'), entry('b')])).toEqual(['message', 'message']);
  });

  it('should continue a message from the same sender soon after', () => {
    const rows = planTimeline(
      timelineOf([messageEntry('a'), messageEntry('b', { timestamp_ms: T + 60_000 })]),
      room,
    );

    expect(rows.map((r) => r.kind === 'message' && r.continues)).toEqual([false, true]);
  });

  it('should start a new group after the group window', () => {
    const rows = planTimeline(
      timelineOf([messageEntry('a'), messageEntry('b', { timestamp_ms: T + GROUP_WINDOW_MS })]),
      room,
    );

    expect(rows.map((r) => r.kind === 'message' && r.continues)).toEqual([false, false]);
  });

  it('should start a new group for another sender', () => {
    const rows = planTimeline(
      timelineOf([messageEntry('a'), messageEntry('b', { sender_id: ADA })]),
      room,
    );

    expect(rows.map((r) => r.kind === 'message' && r.continues)).toEqual([false, false]);
  });

  it('should start a new group for a reply', () => {
    const reply = { event_id: 'a', sender_name: 'Grace', body: 'a', call: null };

    const rows = planTimeline(
      timelineOf([messageEntry('a'), messageEntry('b', { reply_to: reply })]),
      room,
    );

    expect(rows.map((r) => r.kind === 'message' && r.continues)).toEqual([false, false]);
  });

  it('should start a new group after a date divider', () => {
    const divider: TimelineEntry = { id: 'd', kind: { type: 'DateDivider', data: T } };
    const rows = planTimeline(timelineOf([messageEntry('a'), divider, messageEntry('b')]), room);
    expect(rows.map((r) => r.kind === 'message' && r.continues)).toEqual([false, false, false]);
  });

  it('should merge joins in a row into one line', () => {
    const rows = planTimeline(
      timelineOf([
        roomEventEntry('1', ada, { type: 'Joined' }),
        roomEventEntry('2', grace, { type: 'Joined' }),
      ]),
      room,
    );

    expect(rows.map((r) => (r.kind === 'events' ? r.events.length : 0))).toEqual([2]);
  });

  it('should merge joins across hidden entries', () => {
    const hidden: TimelineEntry = { id: 'h', kind: { type: 'Other' } };

    const rows = planTimeline(
      timelineOf([
        roomEventEntry('1', ada, { type: 'Joined' }),
        hidden,
        roomEventEntry('2', grace, { type: 'Joined' }),
      ]),
      room,
    );

    expect(rows.map((r) => (r.kind === 'events' ? r.events.length : 0))).toEqual([2]);
  });

  it('should not merge a join with a leave', () => {
    const entries = [
      roomEventEntry('1', ada, { type: 'Joined' }),
      roomEventEntry('2', grace, { type: 'Left' }),
    ];

    expect(kinds(entries)).toEqual(['events', 'events']);
  });

  it('should not merge other room events', () => {
    const entries = [
      roomEventEntry('1', ada, { type: 'RoomName', data: 'a' }),
      roomEventEntry('2', ada, { type: 'RoomName', data: 'b' }),
    ];

    expect(kinds(entries)).toEqual(['events', 'events']);
  });

  it('should show the read marker as the new row', () => {
    expect(kinds([entry('a'), { id: 'r', kind: { type: 'ReadMarker' } }, entry('b')])).toEqual([
      'message',
      'new',
      'message',
    ]);
  });

  it('should show the start of history once reached', () => {
    expect(kinds([entry('a')], { reachedStart: true })).toEqual(['start', 'message']);
  });

  it('should show the start entry once', () => {
    expect(
      kinds([{ id: 's', kind: { type: 'Start' } }, entry('a')], { reachedStart: true }),
    ).toEqual(['start', 'message']);
  });

  it('should show a page loading above the messages', () => {
    expect(kinds([entry('a')], { loadingOlder: true })).toEqual(['loading', 'message']);
  });

  it('should show a failed page in place of loading', () => {
    expect(kinds([entry('a')], { loadingOlder: true, historyError: 'offline' })).toEqual([
      'historyFailed',
      'message',
    ]);
  });

  it('should leave hidden entries out', () => {
    expect(kinds([{ id: 'h', kind: { type: 'Other' } }])).toEqual([]);
  });

  it('should mark only the newest call card as the latest', () => {
    const call = { type: 'Call' as const, data: { video: true, direct: false, declined_by: [] } };

    const entries = ['a', 'b'].map((id): TimelineEntry => ({
      id,
      kind: { type: 'Message', data: withContent(id, call) },
    }));

    const rows = planTimeline(timelineOf(entries), room);
    expect(rows.map((r) => r.kind === 'message' && r.latestCall)).toEqual([false, true]);
  });

  it('should key rows by their entries', () => {
    expect(planTimeline(timelineOf([entry('a')]), room).map((r) => r.key)).toEqual(['msg-a']);
  });
});

describe('planning a thread', () => {
  const thread = { thread: 'root' };

  it('should show the root from the room until the thread has it', () => {
    const rows = planTimeline(timelineOf([entry('reply')]), { ...thread, root: message('root') });
    expect(rows.map((r) => r.kind)).toEqual(['message', 'replies', 'message']);
  });

  it('should show the thread own root once it has it', () => {
    const rows = planTimeline(timelineOf([entry('root'), entry('reply')]), {
      ...thread,
      root: message('root'),
    });

    expect(rows.map((r) => r.key)).toEqual([
      'thread-msg-root',
      'thread-replies',
      'thread-msg-reply',
    ]);
  });

  it('should start a new group for the first reply', () => {
    const rows = planTimeline(timelineOf([entry('root'), entry('reply')]), thread);
    expect(rows.flatMap((r) => (r.kind === 'message' ? [r.continues] : []))).toEqual([
      false,
      false,
    ]);
  });

  it('should count the replies loaded when the root has no summary', () => {
    const rows = planTimeline(timelineOf([entry('root'), entry('r1'), entry('r2')]), thread);
    expect(rows.find((r) => r.kind === 'replies')).toMatchObject({ count: 2 });
  });

  it('should count the replies the root summary has when more than are loaded', () => {
    const summary = { replies: 7, latest: null, latest_at: null };

    const rows = planTimeline(
      timelineOf([messageEntry('root', { thread: summary }), entry('r1')]),
      thread,
    );

    expect(rows.find((r) => r.kind === 'replies')).toMatchObject({ count: 7 });
  });

  it('should not show the start of history', () => {
    expect(
      planTimeline(timelineOf([entry('root')], { reachedStart: true }), thread).map((r) => r.kind),
    ).toEqual(['message', 'replies']);
  });
});

describe('atStart', () => {
  it('should be at the start once the start entry is in', () => {
    expect(atStart(timelineOf([{ id: 's', kind: { type: 'Start' } }]), null)).toBe(true);
  });

  it('should be at the start of a thread once its root is in', () => {
    expect(atStart(timelineOf([entry('root')]), 'root')).toBe(true);
  });

  it('should not be at the start before either', () => {
    expect(atStart(timelineOf([entry('a')]), 'root')).toBe(false);
  });
});

describe('rowOf', () => {
  it('should find the row of a message', () => {
    const rows = planTimeline(timelineOf([entry('a'), entry('b')]), room);
    expect(rowOf(rows, 'b')).toBe(1);
  });
});

describe('marking read', () => {
  it('should reach the newest message the server has', () => {
    expect(newestSent([entry('a'), messageEntry('b', { pending: true })])).toBe('a');
  });

  it('should mark read when the newest is on screen and new', () => {
    expect(
      shouldMarkRead({
        positioned: true,
        focused: true,
        atLatest: true,
        newest: 'b',
        fullyRead: false,
        lastMarked: marked('a'),
      }),
    ).toBe(true);
  });

  it('should not mark read again for the same message', () => {
    expect(
      shouldMarkRead({
        positioned: true,
        focused: true,
        atLatest: true,
        newest: 'b',
        fullyRead: false,
        lastMarked: marked('b'),
      }),
    ).toBe(false);
  });

  it('should not mark read while scrolled up', () => {
    expect(
      shouldMarkRead({
        positioned: true,
        focused: true,
        atLatest: false,
        newest: 'b',
        fullyRead: false,
        lastMarked: null,
      }),
    ).toBe(false);
  });

  it('should mark read again once the fully-read marker may follow', () => {
    expect(
      shouldMarkRead({
        positioned: true,
        focused: true,
        atLatest: true,
        newest: 'b',
        fullyRead: true,
        lastMarked: marked('b'),
      }),
    ).toBe(true);
  });

  it('should not mark read again once the fully-read marker followed', () => {
    const last = { eventId: 'b', fullyRead: true };
    expect(
      shouldMarkRead({
        positioned: true,
        focused: true,
        atLatest: true,
        newest: 'b',
        fullyRead: true,
        lastMarked: last,
      }),
    ).toBe(false);
  });

  it('should not mark read before the list scrolled to a linked message', () => {
    expect(
      shouldMarkRead({
        positioned: false,
        focused: true,
        atLatest: true,
        newest: 'b',
        fullyRead: false,
        lastMarked: null,
      }),
    ).toBe(false);
  });

  it('should not mark read while another screen is on top', () => {
    expect(
      shouldMarkRead({
        positioned: true,
        focused: false,
        atLatest: true,
        newest: 'b',
        fullyRead: false,
        lastMarked: null,
      }),
    ).toBe(false);
  });
});

describe('the New divider', () => {
  const marker = { id: 'marker', kind: { type: 'ReadMarker' as const } };

  it('should stand before its boundary even after the read marker moved', () => {
    const rows = planTimeline(
      timelineOf([messageEntry('$1'), messageEntry('$2'), marker, messageEntry('$3')]),
      {
        thread: null,
        marker: { boundary: '$2', fading: true },
      },
    );

    expect(rows.map((r) => (r.kind === 'new' ? `new:${r.fading}` : r.key))).toEqual([
      'msg-$1',
      'new:true',
      'msg-$2',
      'msg-$3',
    ]);
  });

  it('should draw no divider when none is shown', () => {
    const rows = planTimeline(timelineOf([messageEntry('$1'), marker, messageEntry('$2')]), {
      thread: null,
      marker: null,
    });

    expect(rows.some((r) => r.kind === 'new')).toBe(false);
  });
});
