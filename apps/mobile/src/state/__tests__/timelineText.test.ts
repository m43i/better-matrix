import type { RoomChange } from '@better-matrix/matrix-native/types';

import {
  clockTime,
  dateLabel,
  eventRunText,
  humanSize,
  inlineName,
  lastReplyText,
  quoteText,
  roomEventText,
  typingText,
} from '../timelineText';
import { person } from '../../test/fixtures';

const at = (y: number, m: number, d: number, h = 12, min = 0) =>
  new Date(y, m - 1, d, h, min).getTime();

const ada = person('@ada:x', 'Ada');

const event = (change: RoomChange, sender = ada) => ({ sender, change });

describe('dates and times', () => {
  it('should format the time as hours and minutes', () => {
    expect(clockTime(at(2025, 11, 13, 9, 5))).toBe('09:05');
  });

  it('should call today today', () => {
    expect(dateLabel(at(2025, 11, 13, 1), at(2025, 11, 13, 23))).toBe('Today');
  });

  it('should call yesterday yesterday', () => {
    expect(dateLabel(at(2025, 11, 12, 23), at(2025, 11, 13, 1))).toBe('Yesterday');
  });

  it('should spell out an older date', () => {
    expect(dateLabel(at(2025, 11, 13), at(2025, 11, 18))).toBe('November 13, 2025');
  });

  it('should say a thread last reply came today', () => {
    expect(lastReplyText(at(2025, 11, 13, 9, 51), at(2025, 11, 13, 10))).toBe(
      'Last reply today at 09:51',
    );
  });
});

describe('room events', () => {
  it('should say who joined', () => {
    expect(roomEventText(event({ type: 'Joined' }))).toBe('Ada joined');
  });

  it('should name whom an invite was for', () => {
    expect(roomEventText(event({ type: 'Invited', data: person('@g:x', 'Grace') }))).toBe(
      'Ada invited Grace',
    );
  });

  it('should say a display name was removed', () => {
    expect(roomEventText(event({ type: 'DisplayName', data: null }))).toBe(
      'Ada removed their display name',
    );
  });

  it('should list everyone who joined in a row', () => {
    const run = ['Ada', 'Grace', 'Linus'].map((name) =>
      event({ type: 'Joined' }, person(`@${name}:x`, name)),
    );

    expect(eventRunText(run)).toBe('Ada, Grace and Linus joined');
  });

  it('should count the rest of a long run', () => {
    const run = ['A', 'B', 'C', 'D', 'E', 'F'].map((name) =>
      event({ type: 'Left' }, person(`@${name}:x`, name)),
    );

    expect(eventRunText(run)).toBe('A, B, C, D and 2 more left');
  });

  it('should read a run by one person as one event', () => {
    expect(eventRunText([event({ type: 'Joined' }), event({ type: 'Joined' })])).toBe('Ada joined');
  });
});

describe('humanSize', () => {
  it('should keep small sizes in bytes', () => {
    expect(humanSize(512)).toBe('512 B');
  });

  it('should use one decimal for larger units', () => {
    expect(humanSize(1_420_000)).toBe('1.4 MB');
  });
});

describe('typingText', () => {
  it('should name one person typing', () => {
    expect(typingText([ada])).toBe('Ada is typing…');
  });

  it('should count everyone past two', () => {
    const people = ['A', 'B', 'C', 'D'].map((n) => person(n, n));
    expect(typingText(people)).toBe('A, B and 2 others are typing…');
  });
});

describe('quotes', () => {
  it('should quote a call by its kind', () => {
    expect(
      quoteText({
        event_id: '$c',
        sender_name: null,
        body: null,
        call: { video: true, direct: false, declined_by: [] },
      }),
    ).toBe('Video call');
  });

  it('should say when the quoted message is not loaded', () => {
    expect(quoteText({ event_id: '$c', sender_name: null, body: null, call: null })).toBe(
      'Original message not loaded',
    );
  });
});

describe('inlineName', () => {
  it('should put a hash before a room name', () => {
    expect(inlineName({ name: 'general', kind: 'Group' })).toBe('#general');
  });
});
