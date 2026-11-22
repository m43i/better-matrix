import {
  completeAt,
  emotePill,
  EVERYONE,
  livePills,
  mentionsOf,
  pillHtml,
  pillText,
  placePills,
  roomPill,
  triggerAt,
  userPill,
} from '../pills';

const GRACE = userPill('@grace:x.org', 'Grace Hopper');

describe('placePills', () => {
  it('should find a pill where its label stands whole', () => {
    expect(placePills('hi @Grace Hopper!', [GRACE])).toEqual([{ pill: GRACE, start: 3, end: 16 }]);
  });

  it('should not find a pill inside a longer word', () => {
    expect(placePills('hi @Grace Hoppers', [GRACE])).toEqual([]);
  });

  it('should give each pill one place, the earliest', () => {
    expect(placePills('@Grace Hopper and @Grace Hopper', [GRACE]).map((p) => p.start)).toEqual([0]);
  });

  it('should place two pills with one label twice', () => {
    expect(placePills('@Grace Hopper and @Grace Hopper', [GRACE, GRACE]).length).toBe(2);
  });
});

describe('livePills', () => {
  it('should drop a pill whose label was edited away', () => {
    expect(livePills('hi @Grace Hop', [GRACE])).toEqual([]);
  });

  it('should keep the pills still standing', () => {
    expect(livePills('hi @Grace Hopper', [GRACE])).toEqual([GRACE]);
  });
});

describe('mentionsOf', () => {
  it('should mention each person once', () => {
    expect(mentionsOf('@Grace Hopper @Grace Hopper', [GRACE, GRACE])).toEqual({
      user_ids: ['@grace:x.org'],
      room: false,
    });
  });

  it('should mention the room for @room', () => {
    expect(mentionsOf('@room hi', [EVERYONE])).toEqual({ user_ids: [], room: true });
  });

  it('should mention nobody for a name typed without a pill', () => {
    expect(mentionsOf('@Grace Hopper', [])).toEqual({ user_ids: [], room: false });
  });
});

describe('pillHtml', () => {
  it('should link a person by matrix.to with their name', () => {
    expect(pillHtml(GRACE)).toBe('<a href="https://matrix.to/#/@grace:x.org">Grace Hopper</a>');
  });

  it('should link a room by matrix.to with its name', () => {
    expect(pillHtml(roomPill('#rust:x.org', 'rust'))).toBe(
      '<a href="https://matrix.to/#/#rust:x.org">#rust</a>',
    );
  });

  it('should write a custom emoji as the engine does', () => {
    expect(pillHtml(emotePill('mxc://x.org/cat', 'blobcat'))).toBe(
      '<img data-mx-emoticon src="mxc://x.org/cat" alt=":blobcat:" title=":blobcat:" height="32" />',
    );
  });

  it('should write @room as plain text', () => {
    expect(pillHtml(EVERYONE)).toBe('@room');
  });
});

describe('pillText', () => {
  it('should say a person by their name, without the @', () => {
    expect(pillText(GRACE)).toBe('Grace Hopper');
  });
});

describe('triggerAt', () => {
  it('should ask for people after @', () => {
    expect(triggerAt('hi @gr', 6)).toEqual({ kind: 'user', query: 'gr', start: 3, end: 6 });
  });

  it('should ask for rooms after #', () => {
    expect(triggerAt('#ru', 3)).toEqual({ kind: 'room', query: 'ru', start: 0, end: 3 });
  });

  it('should ask for emoji after a colon and two letters', () => {
    expect(triggerAt('nice :pa', 8)).toEqual({ kind: 'emoji', query: 'pa', start: 5, end: 8 });
  });

  it('should not ask for emoji after a colon and one letter', () => {
    expect(triggerAt('nice :p', 7)).toBeNull();
  });

  it('should not ask for anything inside a word', () => {
    expect(triggerAt('mail a@b', 8)).toBeNull();
  });

  it('should not take a time for an emoji', () => {
    expect(triggerAt('at 12:30', 8)).toBeNull();
  });

  it('should ask for commands at the start only', () => {
    expect([triggerAt('/sh', 3), triggerAt('a /sh', 5)]).toEqual([
      { kind: 'command', query: 'sh', start: 0, end: 3 },
      null,
    ]);
  });
});

describe('completeAt', () => {
  it('should replace the typed word and put a space after it', () => {
    expect(completeAt('hi @gr', triggerAt('hi @gr', 6)!, '@Grace Hopper')).toEqual({
      text: 'hi @Grace Hopper ',
      selection: { start: 17, end: 17 },
    });
  });

  it('should not double a space already there', () => {
    expect(completeAt('@gr there', triggerAt('@gr there', 3)!, '@Grace').text).toBe('@Grace there');
  });
});
