import { type ComposerState, EMPTY_COMPOSER } from '../composer';
import { storedDraft, withKeptDraft } from '../drafts';
import type { OutgoingText } from '../format';
import { planSend } from '../outgoing';
import {
  type DraftPill,
  emotePill,
  EVERYONE,
  type Pill,
  pillAt,
  roomPill,
  spoilerPill,
  userPill,
} from '../pills';
import { roomKey } from '../timeline';
import { ADA, message } from '../../test/fixtures';

const KEY = roomKey('!r:x');

/** A composer holding `draft`, each of `pills` placed where its label first stands in it. */
function holding(draft: string, ...pills: Pill[]): ComposerState {
  const placed: DraftPill[] = pills.map((pill) => pillAt(pill, draft.indexOf(pill.label)));

  return { ...EMPTY_COMPOSER, draft, pills: placed };
}

/** `composer`'s draft kept, as the store hands it back, and read into a fresh composer. */
function afterRestart(composer: ComposerState): ComposerState {
  const kept = storedDraft(composer);

  if (!kept) throw new Error('nothing kept');

  return withKeptDraft(undefined, { body: kept.body, html: kept.html });
}

/** The text message sending `composer` sends. */
function sent(composer: ComposerState): OutgoingText {
  const command = planSend(composer, ADA, KEY).commands[0];

  if (command?.type !== 'Send' || command.data.message.type !== 'Text')
    throw new Error('no text sent');

  return command.data.message.data;
}

describe('kept drafts', () => {
  it('should keep nothing of a blank draft', () => {
    expect(storedDraft({ ...EMPTY_COMPOSER, draft: '  \n' })).toBeNull();
  });

  it('should keep a draft exactly as typed, markers, escapes and spaces included', () => {
    const typed = '**hi** \\*literal\\* ';
    const kept = storedDraft({ ...EMPTY_COMPOSER, draft: typed });

    expect(kept && withKeptDraft(undefined, kept).draft).toBe(typed);
  });

  it('should keep the draft set aside under an edit, not the edit', () => {
    const editing = {
      ...EMPTY_COMPOSER,
      editing: message('$mine'),
      draft: 'the edit',
      setAside: { draft: 'the draft', replyTo: null, attachments: [], original: 'old' },
    };

    expect(storedDraft(editing)).toEqual({ body: 'the draft', html: null });
  });

  it('should bring a kept draft back into an empty composer', () => {
    expect(withKeptDraft(undefined, { body: 'hi there', html: '<em>hi</em> there' }).draft).toBe(
      '_hi_ there',
    );
  });

  it('should leave a composer the user typed in and cleared again', () => {
    const cleared = { ...EMPTY_COMPOSER, touched: true };

    expect(withKeptDraft(cleared, { body: 'old', html: null })).toBe(cleared);
  });

  it('should leave what was typed meanwhile', () => {
    const typed = { ...EMPTY_COMPOSER, draft: 'new words' };

    expect(withKeptDraft(typed, { body: 'old', html: null })).toBe(typed);
  });

  it('should bring a kept draft back beside a reply picked meanwhile', () => {
    const replying = { ...EMPTY_COMPOSER, replyTo: message('$q') };

    expect(withKeptDraft(replying, { body: 'old', html: null })).toMatchObject({
      draft: 'old',
      replyTo: message('$q'),
    });
  });
});

describe('kept drafts with pills', () => {
  it('should send a kept mention of a person as the mention it was', () => {
    const before = holding('hi @Ada!', userPill('@ada:x.org', 'Ada'));
    const after = sent(afterRestart(before));

    expect(after).toEqual(sent(before));
    expect(after.html).toBe('<p>hi <a href="https://matrix.to/#/@ada:x.org">Ada</a>!</p>');
    expect(after.mentions).toEqual({ user_ids: ['@ada:x.org'], room: false });
  });

  it('should send a kept mention of a room as the link it was', () => {
    const before = holding('see #design', roomPill('!design:x.org', 'design'));
    const after = sent(afterRestart(before));

    expect(after).toEqual(sent(before));
    expect(after.html).toBe('<p>see <a href="https://matrix.to/#/!design:x.org">#design</a></p>');
  });

  it('should send a kept @room as a mention of the room', () => {
    const before = holding('@room lunch?', EVERYONE);
    const after = sent(afterRestart(before));

    expect(after).toEqual(sent(before));
    expect(after.mentions).toEqual({ user_ids: [], room: true });
  });

  it('should send a kept custom emoji as its image', () => {
    const before = holding('yay :party:', emotePill('mxc://x.org/party', 'party'));
    const after = sent(afterRestart(before));

    expect(after).toEqual(sent(before));
    expect(after.html).toContain('<img data-mx-emoticon src="mxc://x.org/party"');
  });

  it('should send a kept spoiler hidden, its words never markers', () => {
    const before = holding('the end: he **dies**||\nsadly', spoilerPill('he **dies**||\nsadly'));
    const after = sent(afterRestart(before));

    expect(after).toEqual(sent(before));
    expect(after.body).toBe('the end: [Spoiler]');
    expect(after.html).toBe(
      '<p>the end: <span data-mx-spoiler>he **dies**||<br />sadly</span></p>',
    );
  });

  it('should send a kept formula as the formula', () => {
    const before = holding('so $$x^2 < y$$ for @Ada', userPill('@ada:x.org', 'Ada'));
    const after = sent(afterRestart(before));

    expect(after).toEqual(sent(before));
    expect(after.html).toContain('data-mx-maths="x^2 &lt; y"');
  });

  it('should keep a pill whose name holds what reads as markup', () => {
    const before = holding('cc @<b>"Bo" & co</b>', userPill('@bo:x.org', '<b>"Bo" & co</b>'));

    expect(sent(afterRestart(before))).toEqual(sent(before));
  });

  it('should keep the same words typed beside a pill as text', () => {
    const before = holding('@Ada said @Ada', userPill('@ada:x.org', 'Ada'));
    const restored = afterRestart(before);

    expect(restored.pills).toEqual([pillAt(userPill('@ada:x.org', 'Ada'), 0)]);
  });

  it('should keep the pills of the draft set aside under an edit', () => {
    const aside = holding('hi @Ada', userPill('@ada:x.org', 'Ada'));

    const editing = {
      ...EMPTY_COMPOSER,
      editing: message('$mine'),
      draft: 'the edit',
      setAside: { ...aside, replyTo: null, attachments: [], original: 'old' },
    };

    expect(sent(afterRestart(editing))).toEqual(sent(aside));
  });

  it('should bring kept pills back under an edit started meanwhile', () => {
    const kept = storedDraft(holding('hi @Ada', userPill('@ada:x.org', 'Ada')));

    const editing = {
      ...EMPTY_COMPOSER,
      editing: message('$mine'),
      draft: 'the edit',
      setAside: { draft: '', replyTo: null, attachments: [], original: 'old' },
    };

    expect(kept && withKeptDraft(editing, kept).setAside?.pills).toEqual([
      pillAt(userPill('@ada:x.org', 'Ada'), 3),
    ]);
  });

  it('should keep a draft without pills as its text alone', () => {
    expect(storedDraft(holding('just words'))).toEqual({ body: 'just words', html: null });
  });

  it('should read kept HTML that doesn’t spell its body as a message’s', () => {
    const kept = { body: 'hi Ada', html: '<p>hi <span data-pill="user:@ada:x.org">Ada</span></p>' };

    expect(withKeptDraft(undefined, kept).draft).toBe('hi Ada');
  });
});
