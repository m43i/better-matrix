import {
  completeAt,
  editPills,
  emotePill,
  EVERYONE,
  livePills,
  mentionsOf,
  pillAt,
  pillHtml,
  pillText,
  placePills,
  roomPill,
  slicePills,
  spoilerPill,
  trackPills,
  triggerAt,
  userPill,
} from '../pills';

const GRACE = userPill('@grace:x.org', 'Grace Hopper');

/** Another member who goes by the same name. */
const OTHER_GRACE = userPill('@grace:y.org', 'Grace Hopper');

describe('placePills', () => {
  it('should find a pill where its label stands whole', () => {
    const grace = pillAt(GRACE, 3);
    expect(placePills('hi @Grace Hopper!', [grace])).toEqual([{ pill: grace, start: 3, end: 16 }]);
  });

  it('should not find a pill inside a longer word', () => {
    expect(placePills('hi @Grace Hoppers', [pillAt(GRACE, 3)])).toEqual([]);
  });

  it('should not find a pill whose words moved away from its place', () => {
    expect(placePills('so hi @Grace Hopper', [pillAt(GRACE, 3)])).toEqual([]);
  });

  it('should place a pill only where it was put, not where its words stand earlier', () => {
    const draft = '@Grace Hopper and @Grace Hopper';
    expect(placePills(draft, [pillAt(GRACE, 18)]).map((p) => p.start)).toEqual([18]);
  });

  it('should place two pills with one label at their own places', () => {
    const draft = '@Grace Hopper and @Grace Hopper';
    const pills = [pillAt(OTHER_GRACE, 18), pillAt(GRACE, 0)];
    expect(placePills(draft, pills).map((p) => [p.pill.id, p.start])).toEqual([
      ['user:@grace:x.org', 0],
      ['user:@grace:y.org', 18],
    ]);
  });
});

describe('placePills with a spoiler', () => {
  it('should find a spoiler inside a longer word', () => {
    const llo = pillAt(spoilerPill('llo'), 2);
    expect(placePills('hello', [llo])).toEqual([{ pill: llo, start: 2, end: 5 }]);
  });
});

describe('livePills', () => {
  it('should drop a pill whose label was edited away', () => {
    expect(livePills('hi @Grace Hop', [pillAt(GRACE, 3)])).toEqual([]);
  });

  it('should keep the pills still standing', () => {
    expect(livePills('hi @Grace Hopper', [pillAt(GRACE, 3)])).toEqual([pillAt(GRACE, 3)]);
  });
});

describe('editPills', () => {
  const pills = [pillAt(GRACE, 0), pillAt(OTHER_GRACE, 20)];

  it('should move the pills after an edit by what it added', () => {
    expect(editPills(pills, 14, 14, 'and').map((p) => p.start)).toEqual([0, 23]);
  });

  it('should drop a pill an edit cut into', () => {
    expect(editPills(pills, 10, 15, '')).toEqual([{ ...pillAt(OTHER_GRACE, 15) }]);
  });
});

describe('editPills on a spoiler', () => {
  // `he dies`: the spoiler `dies` stands from 3 to 7.
  const dies = [pillAt(spoilerPill('dies'), 3)];

  it('should take in what was typed inside it', () => {
    expect(editPills(dies, 5, 5, '||')).toEqual([pillAt(spoilerPill('di||es'), 3)]);
  });

  it('should keep what is left of its words when some are deleted', () => {
    expect(editPills(dies, 3, 5, '')).toEqual([pillAt(spoilerPill('es'), 3)]);
  });

  it('should leave what was typed at either of its ends outside it', () => {
    expect(editPills(dies, 7, 7, '!')).toEqual(dies);
    expect(editPills(dies, 3, 3, 'x')).toEqual([pillAt(spoilerPill('dies'), 4)]);
  });

  it('should keep its words after a cut across its start', () => {
    expect(editPills(dies, 1, 5, 'X')).toEqual([pillAt(spoilerPill('es'), 2)]);
  });

  it('should keep its words before a cut across its end', () => {
    expect(editPills(dies, 5, 9, '')).toEqual([pillAt(spoilerPill('di'), 3)]);
  });

  it('should be gone once all of its words are deleted', () => {
    expect(editPills(dies, 3, 7, '')).toEqual([]);
    expect(editPills(dies, 2, 8, 'x')).toEqual([]);
  });
});

describe('trackPills', () => {
  const before = '@Grace Hopper and @Grace Hopper';
  const pills = [pillAt(GRACE, 0), pillAt(OTHER_GRACE, 18)];

  it('should keep each person’s pill as the words between them are deleted', () => {
    expect(trackPills(before, '@Grace Hopper @Grace Hopper', pills, 14)).toEqual([
      pillAt(GRACE, 0),
      pillAt(OTHER_GRACE, 14),
    ]);
  });

  it('should keep each person’s pill as words are typed before them', () => {
    expect(trackPills(before, `thanks ${before}`, pills, 0)).toEqual([
      pillAt(GRACE, 7),
      pillAt(OTHER_GRACE, 25),
    ]);
  });

  it('should drop the pill deleted, not the other with the same name', () => {
    const two = '@Grace Hopper @Grace Hopper';
    const placed = [pillAt(GRACE, 0), pillAt(OTHER_GRACE, 14)];
    expect(trackPills(two, '@Grace Hopper', placed, 0)).toEqual([pillAt(OTHER_GRACE, 0)]);
  });

  it('should drop a pill whose words were edited', () => {
    expect(trackPills(before, '@Grace Hoper and @Grace Hopper', pills, 10)).toEqual([
      pillAt(OTHER_GRACE, 17),
    ]);
  });

  it('should keep a pill the same words typed elsewhere don’t take over', () => {
    const one = 'hi @Grace Hopper';
    expect(trackPills(one, `@Grace Hopper ${one}`, [pillAt(GRACE, 3)], 0)).toEqual([
      pillAt(GRACE, 17),
    ]);
  });
});

describe('slicePills', () => {
  it('should give the pills within a slice, placed in it', () => {
    const pills = [pillAt(GRACE, 0), pillAt(OTHER_GRACE, 18)];
    expect(slicePills(pills, 14, 31, 2)).toEqual([pillAt(OTHER_GRACE, 6)]);
  });
});

describe('mentionsOf', () => {
  it('should mention each person once', () => {
    expect(
      mentionsOf('@Grace Hopper @Grace Hopper', [pillAt(GRACE, 0), pillAt(GRACE, 14)]),
    ).toEqual({ user_ids: ['@grace:x.org'], room: false });
  });

  it('should mention two people with one name each', () => {
    expect(
      mentionsOf('@Grace Hopper @Grace Hopper', [pillAt(GRACE, 0), pillAt(OTHER_GRACE, 14)]),
    ).toEqual({ user_ids: ['@grace:x.org', '@grace:y.org'], room: false });
  });

  it('should mention the room for @room', () => {
    expect(mentionsOf('@room hi', [pillAt(EVERYONE, 0)])).toEqual({ user_ids: [], room: true });
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

  it('should write a spoiler’s words escaped in its span, line breaks as breaks', () => {
    expect(pillHtml(spoilerPill('a<b\nc'))).toBe('<span data-mx-spoiler>a&lt;b<br />c</span>');
  });
});

describe('pillText', () => {
  it('should say a person by their name, without the @', () => {
    expect(pillText(GRACE)).toBe('Grace Hopper');
  });

  it('should say a spoiler only as [Spoiler]', () => {
    expect(pillText(spoilerPill('he dies'))).toBe('[Spoiler]');
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
