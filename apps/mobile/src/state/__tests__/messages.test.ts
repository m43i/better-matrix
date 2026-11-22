import type { Attachment, CallCard, Poll } from '@better-matrix/matrix-native/types';

import {
  attachmentPreview,
  attachmentActions,
  attachmentRows,
  callStatus,
  cardLink,
  messageActions,
  forwardSections,
  marksMention,
  messageText,
  nextVote,
  playTime,
  pollShowsResults,
  pollStatus,
  pollWinners,
  prepare,
  previewSize,
  editText,
} from '../messages';
import { translationUrl } from '../actions/message-actions';
import { ADA, GRACE, message, roomSummary, withContent } from '../../test/fixtures';

const text = (body: string, html: string | null = null) =>
  withContent('$m', { type: 'Text', data: { body, html } });

describe('prepare', () => {
  it('should parse a message once', () => {
    const m = text('hi');
    expect(prepare(m)).toBe(prepare(m));
  });

  it('a message replaced with the same text should keep its blocks', () => {
    const m = text('**hi**', '<strong>hi</strong>');
    expect(prepare({ ...m, read_receipts: [] }).blocks).toBe(prepare(m).blocks);
  });

  it('an edited message should be parsed again', () => {
    expect(prepare(text('hi')).blocks).not.toBe(prepare(text('hi there')).blocks);
  });

  it('the same words with other formatting should be parsed apart', () => {
    expect(prepare(text('hi', '<em>hi</em>')).blocks).not.toEqual(prepare(text('hi')).blocks);
  });

  it('should find the link that gets a card', () => {
    expect(prepare(text('see https://matrix.org/blog')).link).toBe('https://matrix.org/blog');
  });

  it('should show a GIF link as the GIF with the rest of the words', () => {
    const gif = prepare(text('lol https://media.tenor.com/x/kiwi.gif')).gif;
    expect(gif?.url).toBe('https://media.tenor.com/x/kiwi.gif');
    expect(gif?.rest).toEqual([
      { type: 'paragraph', inlines: [{ type: 'text', text: 'lol', marks: {} }] },
    ]);
  });

  it('should leave no words of a message that is only a GIF link', () => {
    expect(prepare(text('https://media.tenor.com/x/kiwi.gif')).gif?.rest).toBeNull();
  });

  it('should card no link of a message holding a spoiler', () => {
    const spoiled = text(
      'the end: https://example.org/ending',
      'the end: <span data-mx-spoiler>https://example.org/ending</span>',
    );

    expect(prepare(spoiled).link).toBeNull();
  });

  it('should play no GIF of a message holding a spoiler', () => {
    const spoiled = text(
      'https://media.tenor.com/x/kiwi.gif',
      '<span data-mx-spoiler="gross">a kiwi</span>',
    );

    expect(prepare(spoiled).gif).toBeNull();
  });

  it('should still card a link of a message whose formatting holds no spoiler', () => {
    const plain = text('see https://matrix.org/blog', 'see <em>https://matrix.org/blog</em>');

    expect(prepare(plain).link).toBe('https://matrix.org/blog');
  });

  it('should not take words about spoilers for a spoiler', () => {
    const about = text(
      'write data-mx-spoiler: https://spec.matrix.org',
      'write <code>data-mx-spoiler</code>: https://spec.matrix.org',
    );

    expect(prepare(about).link).toBe('https://spec.matrix.org');
  });

  it('should card a GIF link while GIFs do not play', () => {
    expect(cardLink(prepare(text('https://media.tenor.com/x/kiwi.gif')), false)).toBe(
      'https://media.tenor.com/x/kiwi.gif',
    );
  });

  it('should not card a GIF link while GIFs play', () => {
    expect(cardLink(prepare(text('https://media.tenor.com/x/kiwi.gif')), true)).toBeNull();
  });
});

describe('messageText', () => {
  it('should copy formatted text as it reads', () => {
    expect(messageText(text('**hi**', '<b>hi</b>'))).toBe('hi');
  });

  it('should copy a spoiler as [Spoiler], never its words', () => {
    expect(
      messageText(text('the end: [Spoiler]', 'the end: <span data-mx-spoiler>he dies</span>')),
    ).toBe('the end: [Spoiler]');
  });

  it('should translate a spoiler as [Spoiler], never its words', () => {
    const spoiled = text('[Spoiler]', '<span data-mx-spoiler>the butler did it</span>');
    const url = translationUrl(messageText(spoiled) ?? '', 'en');

    expect(url).toContain(encodeURIComponent('[Spoiler]'));
    expect(url).not.toContain('butler');
  });

  it('should copy a spoiler in a formatted emote as [Spoiler]', () => {
    const emote = withContent('$m', {
      type: 'Emote',
      data: { body: 'hides [Spoiler]', html: 'hides <span data-mx-spoiler>it</span>' },
    });

    expect(messageText(emote)).toBe('* Grace hides [Spoiler]');
  });

  it('should have nothing to copy of a deleted message', () => {
    expect(messageText(withContent('$m', { type: 'Redacted' }))).toBeNull();
  });
});

describe('messageActions', () => {
  it('should not copy the filename of an attachment as message text', () => {
    const attachment = withContent('$m', {
      type: 'Attachment',
      data: {
        kind: 'Image',
        name: 'photo.png',
        media: 'original',
        thumbnail: null,
        dimensions: null,
        audio: null,
        size: null,
      },
    });

    expect(messageActions(attachment, ADA, false).copy).toBe(false);
  });

  it('should not offer to copy empty message text', () => {
    expect(messageActions(text('  '), ADA, false).copy).toBe(false);
  });
  it('should let the user delete their own message', () => {
    expect(messageActions({ ...message('$m'), sender_id: ADA }, ADA, false).delete).toBe(true);
  });

  it('should not let the user delete someone else message', () => {
    expect(messageActions(message('$m'), ADA, false).delete).toBe(false);
  });

  it('should offer reporting someone else message', () => {
    expect(messageActions(message('$m'), ADA, false).report).toBe(true);
  });

  it('should not offer reporting the user own message', () => {
    expect(messageActions({ ...message('$m'), sender_id: ADA }, ADA, false).report).toBe(false);
  });

  it('should not offer reactions on a call card', () => {
    const call = withContent('$m', {
      type: 'Call',
      data: { video: null, direct: false, declined_by: [] },
    });

    expect(messageActions(call, ADA, false).react).toBe(false);
  });

  it('should not act on a message still sending', () => {
    const pending = { ...message('$m'), sender_id: ADA, editable: true, pending: true };
    expect(messageActions(pending, ADA, false)).toMatchObject({
      react: false,
      reply: false,
      edit: false,
      delete: false,
    });
  });

  it('should not offer a thread inside a thread', () => {
    expect(messageActions(message('$m'), ADA, true).thread).toBe(false);
  });

  it('should let the sender end an open poll', () => {
    const poll = withContent('$m', { type: 'Poll', data: makePoll(1, []) }, { sender_id: ADA });
    expect(messageActions(poll, ADA, false).endPoll).toBe(true);
  });
});

describe('attachmentActions', () => {
  const file: Attachment = {
    kind: 'Image',
    name: 'photo.png',
    media: 'original',
    thumbnail: null,
    dimensions: null,
    audio: null,
    size: null,
  };

  it('should offer image copying alongside opening sharing and saving', () => {
    expect(attachmentActions(file)).toEqual(['copyImage', 'open', 'share', 'save']);
  });

  it('should not offer image copying for other files', () => {
    expect(attachmentActions({ ...file, kind: 'File', name: 'notes.pdf' })).toEqual([
      'open',
      'share',
      'save',
    ]);
  });

  it('should offer no file actions without the original media', () => {
    expect(attachmentActions({ ...file, media: null })).toEqual([]);
  });

  it('should offer no file actions while the media is held back', () => {
    expect(attachmentActions(file, true)).toEqual([]);
  });

  it('a sheet standing for held-back media alone should offer Show instead of nothing', () => {
    expect(attachmentRows(file, true, true)).toEqual(['show']);
  });

  it('a message sheet should offer nothing for held-back media, so it draws no divider', () => {
    expect(attachmentRows(file, true, false)).toEqual([]);
  });

  it('shown media should offer its actions, not Show', () => {
    expect(attachmentRows(file, false, true)).toEqual(['copyImage', 'open', 'share', 'save']);
  });
});

function makePoll(max: number, mine: string[], change: Partial<Poll> = {}): Poll {
  return {
    question: '?',
    answers: ['a', 'b', 'c'].map((id) => ({ id, text: id, votes: 0, mine: mine.includes(id) })),
    disclosed: true,
    max_selections: max,
    voters: 0,
    ended: false,
    ...change,
  };
}

describe('nextVote', () => {
  it('should move a single-choice vote to the answer pressed', () => {
    expect(nextVote(makePoll(1, ['a']), 'b')).toEqual(['b']);
  });

  it('should take the vote back when pressing one own answer', () => {
    expect(nextVote(makePoll(1, ['a']), 'a')).toEqual([]);
  });

  it('should add to a multiple-choice vote', () => {
    expect(nextVote(makePoll(2, ['a']), 'b')).toEqual(['a', 'b']);
  });

  it('should stop a multiple-choice vote at its limit', () => {
    expect(nextVote(makePoll(2, ['a', 'b']), 'c')).toEqual(['a', 'b']);
  });
});

describe('poll results', () => {
  it('should hide counts of an open undisclosed poll', () => {
    expect(pollShowsResults(makePoll(1, [], { disclosed: false }))).toBe(false);
  });

  it('should show counts once the poll ended', () => {
    expect(pollShowsResults(makePoll(1, [], { disclosed: false, ended: true }))).toBe(true);
  });

  it('should name the answers with the most votes the winners', () => {
    const poll = makePoll(1, [], { ended: true });
    poll.answers = poll.answers.map((a) => ({ ...a, votes: a.id === 'b' ? 3 : 1 }));
    expect([...pollWinners(poll)]).toEqual(['b']);
  });

  it('should name no winner of a poll without votes', () => {
    expect(pollWinners(makePoll(1, [], { ended: true })).size).toBe(0);
  });

  it('should say the results are final once ended', () => {
    expect(pollStatus(makePoll(1, [], { ended: true, voters: 4 }))).toBe('Final results · 4 votes');
  });
});

describe('callStatus', () => {
  const card: CallCard = { video: true, direct: false, declined_by: [] };

  const room = (participants: string[], via: 'Rtc' | null = 'Rtc') => ({
    ...roomSummary('!r'),
    call: { via, participants },
  });

  it('should show the newest card of a call with people in it under way', () => {
    expect(
      callStatus(card, room([GRACE]), { latest: true, callHere: null, ringing: false }),
    ).toMatchObject({
      ongoing: true,
      join: true,
      label: 'Under way · 1 in the call',
    });
  });

  it('should show older cards ended', () => {
    expect(
      callStatus(card, room([GRACE]), { latest: false, callHere: null, ringing: false }),
    ).toMatchObject({
      ongoing: false,
      label: 'Ended',
    });
  });

  it('should not offer to join a call the user is in', () => {
    expect(
      callStatus(card, room([GRACE]), { latest: true, callHere: 'Rtc', ringing: false }),
    ).toMatchObject({
      joined: true,
      join: false,
    });
  });

  it('should not offer to join where the room allows no calls', () => {
    expect(
      callStatus(card, room([GRACE], null), { latest: true, callHere: null, ringing: false }).join,
    ).toBe(false);
  });

  it('should show a ringing one-to-one call ringing', () => {
    const direct = { ...card, direct: true };
    expect(
      callStatus(direct, room([]), { latest: true, callHere: null, ringing: true }).label,
    ).toBe('Ringing…');
  });
});

describe('attachments', () => {
  const image: Attachment = {
    kind: 'Image',
    name: 'a.png',
    size: 10,
    media: 'mxc-a',
    thumbnail: null,
    dimensions: [1440, 720],
    audio: null,
  };

  it('should preview an image from its thumbnail when it has one', () => {
    expect(attachmentPreview({ ...image, thumbnail: 'mxc-t' })).toEqual({
      media: 'mxc-t',
      size: { type: 'Original' },
    });
  });

  it('should preview an image as a server thumbnail otherwise', () => {
    expect(attachmentPreview(image)?.size.type).toBe('Thumbnail');
  });

  it('should not preview a file', () => {
    expect(attachmentPreview({ ...image, kind: 'File' })).toBeNull();
  });

  it('should fit a wide image to the box width', () => {
    expect(previewSize([1440, 720], { width: 360, height: 280 })).toEqual({
      width: 360,
      height: 180,
    });
  });

  it('should not upscale small images', () => {
    expect(previewSize([100, 50], { width: 360, height: 280 })).toEqual({ width: 100, height: 50 });
  });

  it('should read play time as minutes and seconds', () => {
    expect(playTime(75.4)).toBe('1:15');
  });
});

describe('editText', () => {
  it('should bring formatting back as the composer markers', () => {
    const formatted = withContent('$1', {
      type: 'Text',
      data: { body: 'hi there', html: '<strong>hi</strong> there' },
    });

    expect(editText(formatted)).toBe('**hi** there');
  });

  it('should keep plain text as it is', () => {
    expect(editText(withContent('$1', { type: 'Text', data: { body: 'plain', html: null } }))).toBe(
      'plain',
    );
  });
});

describe('message utilities', () => {
  it('should offer forwarding and links for another person’s sent message', () => {
    expect(messageActions(message('$sent'), ADA, false, true)).toMatchObject({
      forward: true,
      link: true,
      pin: true,
      edit: false,
    });
  });
  it('should offer saving a sent message to anyone who can read it', () =>
    expect(messageActions(message('$sent'), ADA, false, false).save).toBe(true));
  it('should not offer saving a message still sending or deleted', () => {
    expect(
      messageActions({ ...message('local:draft'), pending: true }, ADA, false, true).save,
    ).toBe(false);
    expect(messageActions(withContent('$gone', { type: 'Redacted' }), ADA, false, true).save).toBe(
      false,
    );
  });
  it('should hide pinning without room permission', () =>
    expect(messageActions(message('$sent'), ADA, false, false).pin).toBe(false));
  it('should not forward or link local echoes', () =>
    expect(
      messageActions({ ...message('local:draft'), pending: true }, ADA, false, true),
    ).toMatchObject({ forward: false, link: false, pin: false }));
  it('should retain a link but no content actions for a redacted message', () =>
    expect(
      messageActions(withContent('$gone', { type: 'Redacted' }), ADA, false, true),
    ).toMatchObject({ link: true, forward: false, pin: false, translate: false }));
});

it('should separate forwarding destinations into rooms and direct messages', () => {
  const room = roomSummary('!r:x', 'general');
  const direct = roomSummary('!dm:x', 'Grace', 'Direct');
  expect(forwardSections([direct, room], '')).toEqual([
    { kind: 'Group', data: [room] },
    { kind: 'Direct', data: [direct] },
  ]);
});

it('should hide an empty forwarding section after searching', () => {
  const room = roomSummary('!r:x', 'general');
  const direct = roomSummary('!dm:x', 'Grace', 'Direct');
  expect(forwardSections([room, direct], 'gr')).toEqual([{ kind: 'Direct', data: [direct] }]);
});

describe('emotes and places', () => {
  it('should draw an emote as the sender’s name before the action', () => {
    const emote = withContent('$e', { type: 'Emote', data: { body: 'waves', html: null } });
    expect(prepare(emote).blocks).toEqual([
      {
        type: 'paragraph',
        inlines: [
          { type: 'text', text: '* Grace ', marks: { italic: true } },
          { type: 'text', text: 'waves', marks: {} },
        ],
      },
    ]);
  });

  it('should copy an emote with its sender', () => {
    const emote = withContent('$e', { type: 'Emote', data: { body: 'waves', html: null } });
    expect(messageText(emote)).toBe('* Grace waves');
  });

  it('should copy a place by its name', () => {
    const place = withContent('$p', {
      type: 'Location',
      data: { geo_uri: 'geo:1,2', description: 'Home', map: null },
    });

    expect(messageText(place)).toBe('Home');
  });
});

describe('marksMention', () => {
  it('should mark someone else’s message that mentions the user', () => {
    expect(marksMention({ ...message('$a'), highlighted: true }, ADA)).toBe(true);
  });

  it('should not mark the user’s own message, even highlighted', () => {
    expect(marksMention({ ...message('$a'), sender_id: ADA, highlighted: true }, ADA)).toBe(false);
  });

  it('should not mark a message that doesn’t mention the user', () => {
    expect(marksMention(message('$a'), ADA)).toBe(false);
  });
});
