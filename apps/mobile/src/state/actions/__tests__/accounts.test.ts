import { detachedEngine } from '@better-matrix/matrix-native/client';
import type { Command, Homeserver, PendingContact } from '@better-matrix/matrix-native/types';

import { OAUTH_REDIRECT } from '../../../engine/identity';
import { t } from '../../../i18n';
import { ADA, GRACE, summary } from '../../../test/fixtures';
import { Store } from '../../store';
import {
  addContact,
  editableName,
  installAccountEffects,
  nameChanged,
  parseSignInInput,
  removeEnded,
  requestContactCode,
  serverName,
  setStatus,
  SignInFlow,
  signOut,
} from '../accounts';

const flush = () => new Promise((resolve) => setTimeout(resolve, 0));

function setup() {
  const { engine, sent } = detachedEngine();
  const store = new Store(engine);
  const commands = () => sent.map((s) => s.command);
  const last = () => sent[sent.length - 1]!;

  const finish = (request: number, error?: string) =>
    engine.emit({
      type: 'Finished',
      data: { request, result: error === undefined ? { Ok: null } : { Err: error } },
    });

  return { engine, sent, store, commands, last, finish };
}

function homeserver(
  methods: Homeserver['methods'],
  server_name: string | null = 'example.org',
): Homeserver {
  return { server_name, url: 'https://matrix.example.org/', methods };
}

/** A sign-in that discovered `server`, with the browser answering `back`. */
function discovered(server: Homeserver, back: string | null = null) {
  const env = setup();
  const browser = jest.fn((_url: string, _redirect: string) => Promise.resolve(back));
  const flow = new SignInFlow(env.store, browser);
  flow.submitServer('example.org');
  const discover = env.last().request;
  env.engine.emit({
    type: 'HomeserverDiscovered',
    data: { request: discover, homeserver: server },
  });
  env.finish(discover);

  return { ...env, flow, browser };
}

describe('parseSignInInput', () => {
  it('should discover a server name as typed', () => {
    expect(parseSignInInput(' example.org ')).toEqual({ server: 'example.org', username: null });
  });

  it('should drop a trailing slash from a URL', () => {
    expect(parseSignInInput('https://example.org/')?.server).toBe('https://example.org');
  });

  it('should take the server and the username from a Matrix ID', () => {
    expect(parseSignInInput('@alice:example.org')).toEqual({
      server: 'example.org',
      username: 'alice',
    });
  });

  it('should name no server for a blank field', () => {
    expect(parseSignInInput('  ')).toBeNull();
  });
});

describe('serverName', () => {
  it('should prefer the server name', () => {
    expect(serverName(homeserver({ oauth: true, password: false }))).toBe('example.org');
  });

  it('should fall back to the host of the URL', () => {
    expect(serverName(homeserver({ oauth: true, password: false }, null))).toBe(
      'matrix.example.org',
    );
  });
});

describe('SignInFlow', () => {
  it('should ask for a homeserver when the field is blank', () => {
    const { store, sent } = setup();
    const flow = new SignInFlow(store, jest.fn());
    flow.submitServer(' ');
    expect(sent).toEqual([]);
    expect(flow.snapshot().step).toEqual({ type: 'server', error: t('sign_in.enter_server') });
  });

  it('should discover the homeserver typed in', () => {
    const { store, commands } = setup();
    new SignInFlow(store, jest.fn()).submitServer('example.org');
    expect(commands()).toEqual([{ type: 'Discover', data: { server: 'example.org' } }]);
  });

  it('should fill in the username from a full Matrix ID', () => {
    const { store } = setup();
    const flow = new SignInFlow(store, jest.fn());
    flow.submitServer('@alice:example.org');
    expect(flow.snapshot().username).toBe('alice');
  });

  it('should go back to the homeserver with the error when discovery fails', async () => {
    const { store, last, finish } = setup();
    const flow = new SignInFlow(store, jest.fn());
    flow.submitServer('nowhere.example');
    finish(last().request, 'not a homeserver');
    await flush();
    expect(flow.snapshot().step).toEqual({ type: 'server', error: 'not a homeserver' });
  });

  it('should show the password form for a password server', () => {
    const { flow } = discovered(homeserver({ oauth: false, password: true }));
    expect(flow.snapshot().step.type).toBe('password');
  });

  it('should start the browser sign-in for an OAuth server', () => {
    const { last } = discovered(homeserver({ oauth: true, password: true }));
    expect(last().command).toEqual({
      type: 'LoginOAuth',
      data: { homeserver: 'https://matrix.example.org/', resume: null },
    });
  });

  it('should say so when a server offers no way to sign in', () => {
    const { flow } = discovered(homeserver({ oauth: false, password: false }));
    expect(flow.snapshot().step).toEqual({
      type: 'server',
      error: t('sign_in.no_method', { server: 'example.org' }),
    });
  });

  it('should open the sign-in page in a session returning to the app', () => {
    const { engine, last, browser } = discovered(homeserver({ oauth: true, password: false }));
    engine.emit({
      type: 'OpenUrl',
      data: { request: last().request, url: 'https://auth.example.org/login' },
    });
    expect(browser).toHaveBeenCalledWith('https://auth.example.org/login', OAUTH_REDIRECT);
  });

  it('should finish the sign-in with the URL the browser came back with', async () => {
    const back = `${OAUTH_REDIRECT}?code=1&state=2`;

    const { engine, last, commands } = discovered(
      homeserver({ oauth: true, password: false }),
      back,
    );

    const login = last().request;
    engine.emit({
      type: 'OpenUrl',
      data: { request: login, url: 'https://auth.example.org/login' },
    });
    await flush();
    expect(commands()).toContainEqual({ type: 'FinishOAuth', data: { request: login, url: back } });
  });

  it('should cancel the sign-in when the browser is closed without an answer', async () => {
    const { engine, last, commands } = discovered(
      homeserver({ oauth: true, password: false }),
      null,
    );

    const login = last().request;
    engine.emit({
      type: 'OpenUrl',
      data: { request: login, url: 'https://auth.example.org/login' },
    });
    await flush();
    expect(commands()).toContainEqual({ type: 'Cancel', data: login });
  });

  it('should offer to try again without an error after a cancelled browser sign-in', async () => {
    const { last, finish, flow } = discovered(homeserver({ oauth: true, password: false }));
    finish(last().request, 'cancelled');
    await flush();
    expect(flow.snapshot().step).toMatchObject({ type: 'browser', request: null, error: null });
  });

  it('should log in with the username and password', () => {
    const { flow, last } = discovered(homeserver({ oauth: false, password: true }));
    flow.submitPassword(' alice ', 'secret');
    expect(last().command).toEqual({
      type: 'Login',
      data: {
        homeserver: 'https://matrix.example.org/',
        username: 'alice',
        password: 'secret',
        resume: null,
      },
    });
  });

  it('should ask for both fields when the password is missing', () => {
    const { flow, sent } = discovered(homeserver({ oauth: false, password: true }));
    const before = sent.length;
    flow.submitPassword('alice', '');
    expect(sent.length).toBe(before);
    expect(flow.snapshot().step).toMatchObject({ error: t('sign_in.fill_in') });
  });

  it('should keep the password form with the error when the login fails', async () => {
    const { flow, last, finish } = discovered(homeserver({ oauth: false, password: true }));
    flow.submitPassword('alice', 'wrong');
    finish(last().request, 'Invalid password');
    await flush();
    expect(flow.snapshot().step).toMatchObject({
      type: 'password',
      submitting: null,
      error: 'Invalid password',
    });
  });

  it('should put the added account on screen once signed in', async () => {
    const { flow, last, finish, engine, store } = discovered(
      homeserver({ oauth: false, password: true }),
    );

    engine.emit({ type: 'AccountAdded', data: summary(ADA) });
    flow.submitPassword('grace', 'secret');
    engine.emit({ type: 'AccountAdded', data: summary(GRACE, 'Grace') });
    finish(last().request);
    await flush();
    expect(flow.snapshot().step).toEqual({ type: 'done', account: GRACE });
    expect(store.getState().active).toBe(GRACE);
  });

  it('should take back an account signed out softly when signing in as it', () => {
    const { engine, store, last } = setup();
    engine.emit({ type: 'SessionEnded', data: { account: ADA, kept: true } });
    const flow = new SignInFlow(store, jest.fn());
    flow.submitServer(ADA);
    const discover = last().request;
    engine.emit({
      type: 'HomeserverDiscovered',
      data: { request: discover, homeserver: homeserver({ oauth: true, password: false }) },
    });
    expect(last().command).toEqual({
      type: 'LoginOAuth',
      data: { homeserver: 'https://matrix.example.org/', resume: ADA },
    });
  });

  it('should cancel the browser sign-in when switching to the password', () => {
    const { flow, last, commands } = discovered(homeserver({ oauth: true, password: true }));
    const login = last().request;
    flow.usePassword();
    expect(commands()).toContainEqual({ type: 'Cancel', data: login });
    expect(flow.snapshot().step.type).toBe('password');
  });

  it('should cancel what it waits on when the server is changed', () => {
    const { store, last, commands } = setup();
    const flow = new SignInFlow(store, jest.fn());
    flow.submitServer('example.org');
    const discover = last().request;
    flow.changeServer();
    expect(commands()).toContainEqual({ type: 'Cancel', data: discover });
    expect(flow.snapshot().step).toEqual({ type: 'server', error: null });
  });
});

describe('removeEnded', () => {
  it('should forget the data of an account signed out softly', () => {
    const { engine, store, commands } = setup();
    engine.emit({ type: 'SessionEnded', data: { account: ADA, kept: true } });
    removeEnded(store, ADA);
    expect(commands()).toEqual([{ type: 'Logout', data: ADA }]);
    expect(store.getState().ended).toEqual([]);
  });

  it('should only dismiss an account the server signed out fully', () => {
    const { engine, store, sent } = setup();
    engine.emit({ type: 'SessionEnded', data: { account: ADA, kept: false } });
    removeEnded(store, ADA);
    expect(sent).toEqual([]);
    expect(store.getState().ended).toEqual([]);
  });
});

describe('installAccountEffects', () => {
  function withEffects() {
    const env = setup();
    installAccountEffects(env.store);

    return env;
  }

  it('should sign out an account whose session the server ended', () => {
    const { engine, commands } = withEffects();
    engine.emit({ type: 'SessionEnded', data: { account: ADA, kept: false } });
    expect(commands()).toEqual([{ type: 'Logout', data: ADA }]);
  });

  it('should stop a running account signed out softly, keeping its data', () => {
    const { engine, commands } = withEffects();
    engine.emit({ type: 'AccountAdded', data: summary(ADA) });
    engine.emit({ type: 'SessionEnded', data: { account: ADA, kept: true } });
    expect(commands()).toEqual([{ type: 'Suspend', data: ADA }]);
  });

  it('should leave an account suspended in an earlier run as it is', () => {
    const { engine, sent } = withEffects();
    engine.emit({ type: 'SessionEnded', data: { account: ADA, kept: true } });
    expect(sent).toEqual([]);
  });

  it('should stop an ended account only once however often it is reported', () => {
    const { engine, sent } = withEffects();
    engine.emit({ type: 'SessionEnded', data: { account: ADA, kept: false } });
    engine.emit({ type: 'SessionEnded', data: { account: ADA, kept: false } });
    expect(sent.length).toBe(1);
  });

  it('should fetch room keys once the session is verified', () => {
    const { engine, commands } = withEffects();
    engine.emit({
      type: 'SecurityChanged',
      data: { account: ADA, security: { session_verified: false, recovery: 'Enabled' } },
    });
    engine.emit({
      type: 'SecurityChanged',
      data: { account: ADA, security: { session_verified: true, recovery: 'Enabled' } },
    });
    expect(commands()).toEqual([{ type: 'FetchRoomKeys', data: ADA }]);
  });

  it('should not fetch room keys again when nothing changed', () => {
    const { engine, sent } = withEffects();
    const security = { session_verified: true, recovery: 'Enabled' } as const;
    engine.emit({ type: 'SecurityChanged', data: { account: ADA, security } });
    engine.emit({ type: 'SecurityChanged', data: { account: ADA, security } });
    expect(sent.length).toBe(1);
  });
});

describe('profile', () => {
  const pending: PendingContact = {
    kind: 'Email',
    address: 'ada@example.org',
    secret: 's',
    sid: '1',
    submit_url: null,
  };

  it('should leave the name field empty for an account without a display name', () => {
    expect(editableName(ADA, ADA)).toBe('');
  });

  it('should not count a blank name as a change', () => {
    expect(nameChanged('Ada', '  ')).toBe(false);
  });

  it('should count a different name as a change', () => {
    expect(nameChanged('Ada', 'Ada Lovelace')).toBe(true);
  });

  it('should resolve a contact request with the detail to confirm', async () => {
    const { engine, store, last, finish } = setup();
    const asked = requestContactCode(store, ADA, 'Email', ' ada@example.org ', 'DE');
    expect(last().command).toEqual({
      type: 'RequestContactCode',
      data: { account: ADA, kind: 'Email', address: 'ada@example.org', country: 'DE' },
    });
    engine.emit({ type: 'ContactCodeSent', data: { request: last().request, pending } });
    finish(last().request);
    await expect(asked).resolves.toEqual(pending);
  });

  it('should add a contact detail again with the password the server asked for', async () => {
    const { engine, store, sent, finish } = setup();
    const askPassword = jest.fn(() => Promise.resolve('secret'));
    const added = addContact(store, ADA, pending, { askPassword });
    const first = sent[0]!.request;
    engine.emit({ type: 'PasswordNeeded', data: { request: first } });
    finish(first, 'password required');
    await flush();
    finish(sent[1]!.request);
    await expect(added).resolves.toBe(true);
    expect(sent[1]!.command).toEqual<Command>({
      type: 'AddContact',
      data: { account: ADA, pending, password: 'secret' },
    });
  });
});

describe('signOut', () => {
  it('should sign out at once without a pusher on this device', () => {
    const { store, commands } = setup();
    signOut(store, ADA);
    expect(commands()).toEqual<Command[]>([{ type: 'Logout', data: ADA }]);
  });

  it('should remove the pusher of this device before signing out', async () => {
    const { store, commands, sent, finish } = setup();
    store.update((s) => ({
      ...s,
      pushers: { [ADA]: { token: 'ExponentPushToken[a]', gateway: 'https://g', lang: 'en' } },
    }));
    signOut(store, ADA);
    expect(commands()).toEqual<Command[]>([
      {
        type: 'RemovePusher',
        data: {
          account: ADA,
          app_id: 'chat.bettermatrix.mobile',
          push_key: 'ExponentPushToken[a]',
        },
      },
    ]);
    finish(sent[0]!.request, 'gone already');
    await flush();
    await flush();
    expect(commands()[1]).toEqual<Command>({ type: 'Logout', data: ADA });
  });
});

describe('setStatus', () => {
  function withAda() {
    const harness = setup();
    harness.engine.emit({ type: 'AccountAdded', data: summary(ADA) });

    return harness;
  }

  const status = (store: Store) => store.getState().accounts[0]!.status;

  it('should show the chosen status at once and tell the engine', () => {
    const { store, commands } = withAda();
    setStatus(store, ADA, 'Hidden');
    expect(status(store)).toBe('Hidden');
    expect(commands()).toEqual<Command[]>([
      { type: 'SetStatus', data: { account: ADA, status: 'Hidden' } },
    ]);
  });

  it('should send nothing for the status already shown', () => {
    const { store, commands } = withAda();
    setStatus(store, ADA, 'Online');
    expect(commands()).toEqual([]);
  });

  it('should show the previous status and why when the server refuses', async () => {
    const { store, sent, finish } = withAda();
    setStatus(store, ADA, 'Away');
    finish(sent[0]!.request, 'offline');
    await flush();
    expect(status(store)).toBe('Online');
    expect(store.getState().notices.map((n) => n.message)).toContain(
      t('notice.status', { error: 'offline' }),
    );
  });
});
