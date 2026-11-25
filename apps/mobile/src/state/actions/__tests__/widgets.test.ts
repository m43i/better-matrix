import { detachedEngine } from '@better-matrix/matrix-native/client';
import type { WidgetPage } from '@better-matrix/matrix-native/types';

import { ADA, summary } from '../../../test/fixtures';
import { Store } from '../../store';
import type { WidgetTarget } from '../../widgets';
import { openWidgetInBrowser, type PermissionAnswer, WidgetRun } from '../widgets';

/** The capability asking to confirm the user's identity (MSC1960's OpenID). */
const IDENTITY = 'org.matrix.msc1960.openid';

const flush = () => new Promise((resolve) => setTimeout(resolve, 0));

const apiPage: WidgetPage = { url: 'https://w.example/', title: 'Board', bridge: 'WidgetApi' };

const asked = JSON.stringify({ api: 'fromWidget', action: 'content_loaded', requestId: '1' });

/** The run's bridge token in these tests. */
const TOKEN = 'token-1';

/** What the page's bridge script hands the app for the message `json`. */
const envelope = (json: string, token = TOKEN) =>
  JSON.stringify({ token, message: JSON.parse(json) });

/** Where the page's main frame posts from. */
const PAGE_ORIGIN = 'https://w.example';

/** The page's address. */
const PAGE_URL = 'https://w.example/';

function setup(target: WidgetTarget = { type: 'widget', widgetId: 'w1' }, askHosts = true) {
  const { engine, sent } = detachedEngine();
  const store = new Store(engine);
  engine.emit({ type: 'AccountAdded', data: summary(ADA) });
  const run = new WidgetRun(store, ADA, '!r:x', target, TOKEN, askHosts);
  run.start();
  const request = sent[0]!.request;

  const show = (page: WidgetPage = apiPage) =>
    engine.emit({ type: 'WidgetPage', data: { request, page } });

  const message = (text: string) =>
    engine.emit({ type: 'WidgetMessage', data: { request, message: text } });

  const finish = (error?: string) =>
    engine.emit({
      type: 'Finished',
      data: { request, result: error === undefined ? { Ok: null } : { Err: error } },
    });

  const commands = () => sent.slice(1).map((s) => s.command);

  /** The page's bridge script reporting its document, from the page's origin. */
  const report = (lifecycle: 'loading' | 'loaded', from = PAGE_ORIGIN) =>
    run.fromPage(JSON.stringify({ token: TOKEN, lifecycle }), from);

  /** A document at `url` loading in the main frame, as the web view and its script report it. */
  const load = (url = PAGE_URL) => {
    run.pageAt(url);
    report('loading', url);
    report('loaded', url);
  };

  /** What the run posts into the page. */
  const posted: string[] = [];
  run.attach((text) => posted.push(text));

  return {
    engine,
    store,
    run,
    request,
    show,
    message,
    finish,
    commands,
    sent,
    report,
    load,
    posted,
  };
}

describe('WidgetRun', () => {
  it('should run the widget in the app', () => {
    const { sent } = setup();
    expect(sent[0]?.command).toEqual({
      type: 'RunWidget',
      data: { account: ADA, room_id: '!r:x', widget_id: 'w1', browser: false },
    });
  });

  it('should wait for the page before showing anything', () => {
    expect(setup().run.snapshot().view).toEqual({ type: 'starting' });
  });

  it('should show the page the engine reports', () => {
    const { run, show } = setup();
    show();
    expect(run.snapshot().view).toEqual({ type: 'page', page: apiPage, load: 1 });
  });

  it('should load the page again when the engine shows it again, its document gone', () => {
    const { run, show, message, load, report, posted } = setup();
    show();
    load();
    report('loading');
    message('{"n":0}');
    show();
    // What waited for the first load is gone, and the new one waits for its own document.
    message('{"n":1}');
    const view = run.snapshot().view;
    expect([view.type === 'page' && view.load, posted]).toEqual([2, []]);
    load();
    expect(posted).toEqual(['{"n":1}']);
  });

  it('should keep the engine’s messages until the page has loaded, then post them in order', () => {
    const { show, message, load, posted } = setup();
    show();
    message('{"n":1}');
    message('{"n":2}');
    load();
    expect(posted).toEqual(['{"n":1}', '{"n":2}']);
  });

  it('should post the engine’s messages at once into a loaded page', () => {
    const { show, message, load, posted } = setup();
    show();
    load();
    message('{"n":3}');
    expect(posted).toEqual(['{"n":3}']);
  });

  it('should keep messages again while a new document loads', () => {
    const { show, message, load, report, posted } = setup();
    show();
    load();
    report('loading');
    message('{"n":4}');
    expect(posted).toEqual([]);
  });

  it('should keep the engine’s messages from a document of another origin', () => {
    const { run, show, message, load, posted } = setup();
    show();
    load();
    run.pageAt('https://evil.example/');
    message('{"n":5}');
    expect(posted).toEqual([]);
  });

  it('should not take another origin’s document reporting itself loaded', () => {
    const { run, show, message, report, posted } = setup();
    show();
    run.pageAt('https://evil.example/');
    report('loaded', 'https://evil.example');
    message('{"n":6}');
    expect(posted).toEqual([]);
  });

  it('should post what waited once a document of the page’s origin loads', () => {
    const { run, show, message, load, posted } = setup();
    show();
    run.pageAt('https://evil.example/');
    message('{"n":7}');
    load();
    expect(posted).toEqual(['{"n":7}']);
  });

  it('should keep posting after the History API changes the page’s address', () => {
    const { run, show, message, load, posted } = setup();
    show();
    load();
    // Android reports `history.pushState` as a load starting, and no finish.
    run.pageAt(`${PAGE_URL}room`);
    message('{"n":8}');
    expect(posted).toEqual(['{"n":8}']);
  });

  it('should drop what waited when the run ends', async () => {
    const { show, message, finish, load, posted } = setup();
    show();
    message('{"n":9}');
    finish();
    await flush();
    load();
    expect(posted).toEqual([]);
  });

  it('should pass a message the page posted to the engine', () => {
    const { run, show, commands, request } = setup();
    show();
    run.fromPage(envelope(asked), PAGE_ORIGIN);
    expect(commands()).toEqual([
      { type: 'Answer', data: { request, answer: { type: 'WidgetMessage', data: asked } } },
    ]);
  });

  it('should say nothing when a page’s message reaches a run that just ended', () => {
    const { engine, store, run, show, sent } = setup();
    show();
    run.fromPage(envelope(asked), PAGE_ORIGIN);
    const answer = sent[1]!.request;
    engine.emit({ type: 'Finished', data: { request: answer, result: { Err: 'not running' } } });
    expect(store.getState().notices).toEqual([]);
  });

  it('should keep a message from another origin from the engine', () => {
    const { run, show, commands } = setup();
    show();
    run.fromPage(envelope(asked), 'https://ads.example');
    expect(commands()).toEqual([]);
  });

  it('should keep a message without the run’s token from the engine', () => {
    const { run, show, commands } = setup();
    show();
    run.fromPage(envelope(asked, 'guessed'), PAGE_ORIGIN);
    expect(commands()).toEqual([]);
  });

  it('should tell the engine each document its script reports, as it starts and loads', () => {
    const { show, commands, request, load } = setup();
    show();
    load();
    load(`${PAGE_URL}next`);
    expect(commands()).toEqual(
      ['WidgetLoading', 'WidgetLoaded', 'WidgetLoading', 'WidgetLoaded'].map((type) => ({
        type: 'Answer',
        data: { request, answer: { type } },
      })),
    );
  });

  it('should not take the web view’s own load events for documents', () => {
    const { run, show, commands } = setup();
    show();
    run.pageAt(PAGE_URL);
    run.pageAt(`${PAGE_URL}room`);
    expect(commands()).toEqual([]);
  });

  it('should not take a report without the run’s token', () => {
    const { run, show, commands } = setup();
    show();
    run.fromPage(JSON.stringify({ token: 'guessed', lifecycle: 'loading' }), PAGE_ORIGIN);
    expect(commands()).toEqual([]);
  });

  it('should stop the run and say why when the page failed to load', () => {
    const { run, show, commands, request } = setup();
    show();
    run.pageFailed('HTTP 404');
    expect([commands(), run.snapshot().view]).toEqual([
      [{ type: 'Cancel', data: request }],
      { type: 'failed', error: 'HTTP 404' },
    ]);
  });

  it('should close a permission question the engine withdrew, without answering', () => {
    const { engine, run, request, commands } = setup();
    const permissions = [{ capability: 'c1', kind: 'Other' as const, subject: null }];

    engine.emit({
      type: 'WidgetPermissions',
      data: { request, question: 3, widget: 'Board', permissions },
    });

    engine.emit({
      type: 'WidgetPermissions',
      data: { request, question: 3, widget: 'Board', permissions: [] },
    });

    expect([run.snapshot().asking, commands()]).toEqual([null, []]);
  });

  it('should keep the question shown when the engine withdraws another', () => {
    const { engine, run, request } = setup();
    const permissions = [{ capability: 'c1', kind: 'Other' as const, subject: null }];

    engine.emit({
      type: 'WidgetPermissions',
      data: { request, question: 4, widget: 'Board', permissions },
    });

    engine.emit({
      type: 'WidgetPermissions',
      data: { request, question: 3, widget: 'Board', permissions: [] },
    });

    expect(run.snapshot().asking?.question).toBe(4);
  });

  it('should leave for the browser when the engine opens the widget there', () => {
    const { engine, run, request } = setup();
    engine.emit({ type: 'OpenUrl', data: { request, url: 'http://plain.example/' } });
    expect(run.snapshot().view).toEqual({ type: 'browser', url: 'http://plain.example/' });
  });

  it('should stay leaving for the browser once the run finishes', async () => {
    const { engine, run, request, finish } = setup();
    engine.emit({ type: 'OpenUrl', data: { request, url: 'http://plain.example/' } });
    finish();
    await flush();
    expect(run.snapshot().view.type).toBe('browser');
  });

  it('should start over when the main frame leaves the page’s origin', () => {
    const { engine, run, request, show, load, commands } = setup();
    show();
    load();
    const permissions = [{ capability: 'c1', kind: 'Other' as const, subject: null }];

    engine.emit({
      type: 'WidgetPermissions',
      data: { request, question: 1, widget: 'Board', permissions },
    });

    run.pageAt('https://evil.example/');

    expect([commands().at(-1), run.snapshot().asking]).toEqual([
      { type: 'Answer', data: { request, answer: { type: 'WidgetLoading' } } },
      null,
    ]);
  });

  it('should start over once when the main frame stays away from the page’s origin', () => {
    const { run, show, load, commands } = setup();
    show();
    load();
    run.pageAt('https://evil.example/');
    run.pageAt('https://evil.example/more');
    expect(commands().length).toBe(3);
  });

  describe('a meeting’s camera and microphone', () => {
    const meeting: WidgetPage = { url: 'https://bbb.example/join', title: 'Class', bridge: 'None' };

    it('should be bound to the meeting’s own host', () => {
      const { run, show } = setup();
      show(meeting);
      expect(run.snapshot().captureHost).toBe('bbb.example');
    });

    it('should be bound to no host for a widget that isn’t a meeting', () => {
      const { run, show } = setup();
      show();
      expect(run.snapshot().captureHost).toBeNull();
    });

    it('should ask once about another host the meeting moves to', () => {
      const { run, show } = setup();
      show(meeting);
      run.pageAt('https://bbb-2.example/client');
      expect([run.snapshot().captureHost, run.snapshot().askingCapture]).toEqual([
        'bbb.example',
        'bbb-2.example',
      ]);
    });

    it('should be bound to a host the user allowed', () => {
      const { run, show } = setup();
      show(meeting);
      run.pageAt('https://bbb-2.example/client');
      run.answerCapture('bbb-2.example', true);
      expect([run.snapshot().captureHost, run.snapshot().askingCapture]).toEqual([
        'bbb-2.example',
        null,
      ]);
    });

    it('should stay bound to its host after a denial, without asking again', () => {
      const { run, show } = setup();
      show(meeting);
      run.pageAt('https://bbb-2.example/client');
      run.answerCapture('bbb-2.example', false);
      run.pageAt('https://bbb.example/join');
      run.pageAt('https://bbb-2.example/client');
      expect([run.snapshot().captureHost, run.snapshot().askingCapture]).toEqual([
        'bbb.example',
        null,
      ]);
    });

    it('should not ask where the web view asks for each site itself (iOS)', () => {
      const { run, show } = setup({ type: 'widget', widgetId: 'w1' }, false);
      show(meeting);
      run.pageAt('https://bbb-2.example/client');
      expect(run.snapshot().askingCapture).toBeNull();
    });
  });

  it('should keep a message its bridge doesn’t pass from the engine', () => {
    const { run, show, commands } = setup();
    show({ ...apiPage, bridge: 'None' });
    run.fromPage(envelope(asked), PAGE_ORIGIN);
    expect(commands()).toEqual([]);
  });

  it('should ask for the permissions the widget wants', () => {
    const { engine, run, request } = setup();
    const permissions = [{ capability: 'c1', kind: 'Other' as const, subject: null }];

    engine.emit({
      type: 'WidgetPermissions',
      data: { request, question: 1, widget: 'Board', permissions },
    });

    expect(run.snapshot().asking).toEqual({ request, question: 1, widget: 'Board', permissions });
  });

  it('should grant only what was asked for, and stop asking', () => {
    const { engine, run, request, commands } = setup();

    const permissions = ['c1', 'c2'].map((capability) => ({
      capability,
      kind: 'Other' as const,
      subject: null,
    }));

    engine.emit({
      type: 'WidgetPermissions',
      data: { request, question: 2, widget: 'Board', permissions },
    });

    const ask = run.snapshot().asking;

    if (ask) run.grant(ask, ['c2', 'c9']);
    const answer = { type: 'WidgetPermissions', data: { question: 2, granted: ['c2'] } };
    expect([commands(), run.snapshot().asking]).toEqual([
      [{ type: 'Answer', data: { request, answer } }],
      null,
    ]);
  });

  it('should not take an answer from a sheet shown for an earlier question', () => {
    const { engine, run, request, commands } = setup();
    const permissions = [{ capability: 'c1', kind: 'Other' as const, subject: null }];

    engine.emit({
      type: 'WidgetPermissions',
      data: { request, question: 1, widget: 'Board', permissions },
    });

    const earlier = run.snapshot().asking;

    engine.emit({
      type: 'WidgetPermissions',
      data: { request, question: 2, widget: 'Board', permissions },
    });

    if (earlier) run.grant(earlier, ['c1']);
    expect([commands(), run.snapshot().asking?.question]).toEqual([[], 2]);
  });

  it('should not take an answer from a sheet shown for an earlier run', () => {
    const { engine, run, request, commands, sent } = setup();
    const permissions = [{ capability: 'c1', kind: 'Other' as const, subject: null }];

    engine.emit({
      type: 'WidgetPermissions',
      data: { request, question: 1, widget: 'Board', permissions },
    });

    const earlier = run.snapshot().asking;
    run.start();
    const again = sent.at(-1)!.request;

    engine.emit({
      type: 'WidgetPermissions',
      data: { request: again, question: 1, widget: 'Board', permissions },
    });

    if (earlier) run.grant(earlier, ['c1']);
    expect(commands().filter((c) => c.type === 'Answer')).toEqual([]);
  });

  describe('installing the bridge when its script didn’t run', () => {
    it('should install it once a document of the page’s origin loaded without a report', () => {
      const { run, show } = setup();
      show();
      run.pageStarted(PAGE_URL);
      expect(run.pageEnded(PAGE_URL)).toBe(true);
    });

    it('should not install it again in a document that reported itself', () => {
      const { run, show, report } = setup();
      show();
      run.pageStarted(PAGE_URL);
      report('loading');
      expect(run.pageEnded(PAGE_URL)).toBe(false);
    });

    it('should not install it in a document of another origin', () => {
      const { run, show } = setup();
      show();
      run.pageStarted('https://evil.example/');
      expect(run.pageEnded('https://evil.example/')).toBe(false);
    });

    it('should install nothing in a meeting, which has no bridge', () => {
      const { run, show } = setup();
      show({ url: 'https://meet.example/x', title: 'Standup', bridge: 'None' });
      run.pageStarted('https://meet.example/x');
      expect(run.pageEnded('https://meet.example/x')).toBe(false);
    });
  });

  it('should show the integration manager’s terms, then agree to them', () => {
    const { engine, run, request, commands } = setup({ type: 'integrations' });
    const policies = [{ name: 'Terms', url: 'https://im.example/terms' }];
    engine.emit({ type: 'IntegrationTerms', data: { request, policies } });
    expect(run.snapshot().view).toEqual({ type: 'terms', policies });
    run.agree();
    expect([commands(), run.snapshot().view]).toEqual([
      [{ type: 'Answer', data: { request, answer: { type: 'Agree' } } }],
      { type: 'starting' },
    ]);
  });

  it('should cancel the run when the screen goes', () => {
    const { run, request, commands } = setup();
    run.cancel();
    expect(commands()).toEqual([{ type: 'Cancel', data: request }]);
  });

  it('should end when the widget closed itself', async () => {
    const { run, finish } = setup();
    finish();
    await flush();
    expect(run.snapshot().view).toEqual({ type: 'ended' });
  });

  it('should say why it failed', async () => {
    const { run, finish } = setup();
    finish('widget not found');
    await flush();
    expect(run.snapshot().view).toEqual({ type: 'failed', error: 'widget not found' });
  });

  it('should hear nothing more of a run it cancelled', () => {
    const { run, show } = setup();
    run.cancel();
    show();
    expect(run.snapshot().view).toEqual({ type: 'starting' });
  });
});

describe('opening a widget in the browser', () => {
  function browse(answer: PermissionAnswer = true) {
    const { engine, sent } = detachedEngine();
    const store = new Store(engine);
    const opened: string[] = [];
    const asked: string[] = [];

    const done = openWidgetInBrowser(
      store,
      ADA,
      '!r:x',
      'w1',
      (url) => opened.push(url),
      async (widget) => {
        asked.push(widget);

        return answer;
      },
    );

    const request = sent[0]!.request;

    const askIdentity = () =>
      engine.emit({
        type: 'WidgetPermissions',
        data: {
          request,
          question: 1,
          widget: 'Standup',
          permissions: [
            { capability: IDENTITY, kind: 'Identity', subject: 'https://jitsi.example' },
          ],
        },
      });

    const commands = () => sent.slice(1).map((s) => s.command);

    return { engine, sent, done, opened, asked, request, askIdentity, commands };
  }

  it('should open the address the engine gives', async () => {
    const { engine, sent, done, opened, request } = browse();
    expect(sent[0]?.command).toEqual({
      type: 'RunWidget',
      data: { account: ADA, room_id: '!r:x', widget_id: 'w1', browser: true },
    });
    engine.emit({ type: 'OpenUrl', data: { request, url: 'https://meet.example/x?jwt=1' } });
    engine.emit({ type: 'Finished', data: { request, result: { Ok: null } } });
    await done;
    expect(opened).toEqual(['https://meet.example/x?jwt=1']);
  });

  it('should grant what the meeting server asked for once allowed', async () => {
    const { askIdentity, commands, request, asked } = browse(true);
    askIdentity();
    await flush();
    expect([asked, commands()]).toEqual([
      ['Standup'],
      [
        {
          type: 'Answer',
          data: {
            request,
            answer: { type: 'WidgetPermissions', data: { question: 1, granted: [IDENTITY] } },
          },
        },
      ],
    ]);
  });

  it('should grant nothing once denied', async () => {
    const { askIdentity, commands, request } = browse(false);
    askIdentity();
    await flush();
    expect(commands()).toEqual([
      {
        type: 'Answer',
        data: {
          request,
          answer: { type: 'WidgetPermissions', data: { question: 1, granted: [] } },
        },
      },
    ]);
  });

  it('should drop the answer to a question the engine withdrew', async () => {
    const { engine, askIdentity, commands, request } = browse(true);
    askIdentity();
    engine.emit({
      type: 'WidgetPermissions',
      data: { request, question: 1, widget: 'Standup', permissions: [] },
    });
    await flush();
    expect(commands()).toEqual([]);
  });

  it('should drop the answer to a question a newer one replaced', async () => {
    const { engine, request, commands } = browse(true);
    const permissions = [{ capability: IDENTITY, kind: 'Identity' as const, subject: null }];

    engine.emit({
      type: 'WidgetPermissions',
      data: { request, question: 1, widget: 'Standup', permissions },
    });

    engine.emit({
      type: 'WidgetPermissions',
      data: { request, question: 2, widget: 'Standup', permissions },
    });

    await flush();

    const answered = commands().map((c) =>
      c.type === 'Answer' && c.data.answer.type === 'WidgetPermissions'
        ? c.data.answer.data.question
        : null,
    );

    expect(answered).toEqual([2]);
  });

  it('should still answer a question when the engine withdraws another', async () => {
    const { engine, askIdentity, commands, request } = browse(true);
    askIdentity();

    engine.emit({
      type: 'WidgetPermissions',
      data: { request, question: 7, widget: 'Standup', permissions: [] },
    });

    await flush();
    expect(commands().length).toBe(1);
  });

  it('should cancel the run when the question was dismissed', async () => {
    const { askIdentity, commands, request } = browse(null);
    askIdentity();
    await flush();
    expect(commands()).toEqual([{ type: 'Cancel', data: request }]);
  });
});
