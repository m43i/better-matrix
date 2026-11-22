import { type DraftPill, EVERYONE, pillAt, spoilerPill, userPill } from '../pills';
import { commandSuggestions, slashOf } from '../slash';

const CONTEXT = { account: '@ada:x.org', roomId: '!r:x.org' };

const ACCOUNT_ROOM = { account: CONTEXT.account, room_id: CONTEXT.roomId };

const NOBODY = { user_ids: [], room: false };

const GRACE = userPill('@grace:x.org', 'Grace');

/** Grace's pill where `@Grace` first stands in `draft`, as a picked suggestion puts it. */
const graceIn = (draft: string): DraftPill[] =>
  draft.includes(GRACE.label) ? [pillAt(GRACE, draft.indexOf(GRACE.label))] : [];

const slash = (draft: string, pills = graceIn(draft)) => slashOf(draft, pills, CONTEXT);

describe('slashOf', () => {
  it('should leave a draft that is no command as text', () => {
    expect([slash('hello'), slash('/ hi'), slash('/path/to/file')]).toEqual([null, null, null]);
  });

  it('should send /me as an emote', () => {
    expect(slash('/me waves')).toEqual({
      type: 'message',
      message: { type: 'Emote', data: { body: 'waves', html: null, mentions: NOBODY } },
    });
  });

  it('should send an emote mentioning whom its pills name', () => {
    expect(slash('/me waves at @Grace')).toMatchObject({
      message: { data: { mentions: { user_ids: ['@grace:x.org'] } } },
    });
  });

  it('should put a shrug before the message', () => {
    expect(slash('/shrug no idea')).toEqual({
      type: 'message',
      message: {
        type: 'Text',
        data: { body: '¯\\_(ツ)_/¯ no idea', html: null, mentions: NOBODY },
      },
    });
  });

  it('should send a table flip alone', () => {
    expect(slash('/tableflip')).toMatchObject({ message: { data: { body: '(╯°□°)╯︵ ┻━┻' } } });
  });

  it('should send /plain without formatting', () => {
    expect(slash('/plain **not bold**')).toMatchObject({
      message: { type: 'Text', data: { body: '**not bold**', html: null } },
    });
  });

  it('should send a spoiler’s words in /plain as [Spoiler]', () => {
    expect(slash('/plain he dies', [pillAt(spoilerPill('dies'), 10)])).toMatchObject({
      message: { data: { body: 'he [Spoiler]', html: null } },
    });
  });

  it('should send a spoiler inside /spoiler as its words, one spoiler', () => {
    expect(slash('/spoiler he dies', [pillAt(spoilerPill('dies'), 12)])).toMatchObject({
      message: { data: { html: '<p><span data-mx-spoiler>he dies</span></p>' } },
    });
  });

  it('should send /spoiler hidden', () => {
    expect(slash('/spoiler the end')).toMatchObject({
      message: {
        data: { body: '[Spoiler]', html: '<p><span data-mx-spoiler>the end</span></p>' },
      },
    });
  });

  it.each([
    ['several lines', '/spoiler he\ndies\n\nsadly', 'he<br />dies<br /><br />sadly'],
    ['indented lines', '/spoiler he\n    - dies\n  ok', 'he<br />    - dies<br />  ok'],
    ['its own markers', '/spoiler a||b|| **c** ||', 'a||b|| **c** ||'],
    ['HTML', '/spoiler <b>x</b> & y', '&lt;b&gt;x&lt;/b&gt; &amp; y'],
  ])('should send /spoiler with %s hidden whole, as written', (_, draft, hidden) => {
    expect(slash(draft)).toEqual({
      type: 'message',
      message: {
        type: 'Text',
        data: {
          body: '[Spoiler]',
          html: `<p><span data-mx-spoiler>${hidden}</span></p>`,
          mentions: NOBODY,
        },
      },
    });
  });

  it('should send a mention in a spoiler as a link inside it', () => {
    expect(slash('/spoiler @Grace did it')).toMatchObject({
      message: {
        data: {
          body: '[Spoiler]',
          html: '<p><span data-mx-spoiler><a href="https://matrix.to/#/@grace:x.org">Grace</a> did it</span></p>',
          mentions: { user_ids: ['@grace:x.org'] },
        },
      },
    });
  });

  it('should not mention the room for @room hidden by /spoiler', () => {
    expect(slash('/spoiler @room lunch', [pillAt(EVERYONE, 9)])).toMatchObject({
      message: { data: { body: '[Spoiler]', mentions: NOBODY } },
    });
  });

  it('should keep a pill where it stood after the command’s name', () => {
    expect(slash('/shrug ask @Grace')).toMatchObject({
      message: {
        data: { body: '¯\\_(ツ)_/¯ ask Grace', mentions: { user_ids: ['@grace:x.org'] } },
      },
    });
  });

  it('should not take the same words typed without a pill for a mention', () => {
    expect(slash('/me waves at @Grace', [])).toMatchObject({
      message: { data: { body: 'waves at @Grace', mentions: NOBODY } },
    });
  });

  it('should send // as text starting with a slash', () => {
    expect(slash('//shrug')).toMatchObject({
      message: { type: 'Text', data: { body: '/shrug', html: null } },
    });
  });

  it('should invite the person a pill names', () => {
    expect(slash('/invite @Grace')).toEqual({
      type: 'command',
      command: { type: 'Invite', data: { ...ACCOUNT_ROOM, user_id: '@grace:x.org' } },
      after: 'stay',
    });
  });

  it('should invite a typed user ID', () => {
    expect(slash('/invite @kai:x.org', [])).toMatchObject({
      command: { data: { user_id: '@kai:x.org' } },
    });
  });

  it('should join a room by its alias and open it', () => {
    expect(slash('/join #rust:x.org')).toEqual({
      type: 'command',
      command: {
        type: 'JoinRoom',
        data: { account: CONTEXT.account, room_id: '#rust:x.org', via: [] },
      },
      after: 'joined',
    });
  });

  it('should join a room by an ID without a server, as room version 12 writes them', () => {
    expect(slash('/join !31hneApxJ_1o-63DmFrpeqnkFfWppnzWso1JvH3ogLM', [])).toMatchObject({
      command: { data: { room_id: '!31hneApxJ_1o-63DmFrpeqnkFfWppnzWso1JvH3ogLM', via: [] } },
    });
  });

  it('should join the room a pasted link names, through the servers it names', () => {
    expect(slash('/join https://matrix.to/#/!abc?via=x.org&via=y.org', [])).toMatchObject({
      command: { data: { room_id: '!abc', via: ['x.org', 'y.org'] } },
    });
  });

  it('should not join a user ID', () => {
    expect(slash('/join @kai:x.org', [])).toMatchObject({ type: 'usage', name: 'join' });
  });

  it('should leave the room and its screen', () => {
    expect(slash('/leave')).toEqual({
      type: 'command',
      command: { type: 'LeaveRoom', data: { ...ACCOUNT_ROOM, forget: false } },
      after: 'left',
    });
  });

  it('should set the topic', () => {
    expect(slash('/topic Release day')).toMatchObject({
      command: { type: 'SetTopic', data: { ...ACCOUNT_ROOM, topic: 'Release day' } },
    });
  });

  it('should change the display name', () => {
    expect(slash('/nick Ada L.')).toMatchObject({
      command: { type: 'SetDisplayName', data: { account: CONTEXT.account, name: 'Ada L.' } },
    });
  });

  it('should say a command it doesn’t know', () => {
    expect(slash('/frobnicate now')).toEqual({ type: 'unknown', name: 'frobnicate' });
  });

  it('should say what a command lacks', () => {
    expect(slash('/invite')).toEqual({
      type: 'usage',
      name: 'invite',
      args: 'mobile.composer.command_args_user',
    });
  });
});

describe('commandSuggestions', () => {
  it('should suggest the commands starting with what was typed', () => {
    expect(commandSuggestions('t').map((c) => c.name)).toEqual(['tableflip', 'topic']);
  });

  it('should suggest every command for a bare slash', () => {
    expect(commandSuggestions('').length).toBe(12);
  });
});
