import type {
  Widget,
  WidgetBridge,
  WidgetPage,
  WidgetPermission,
} from '@better-matrix/matrix-native/types';

import { ADA, roomSummary } from '../../test/fixtures';
import type { Json } from '../json';
import {
  bridgeScript,
  captureGrant,
  httpFailure,
  loadFailure,
  pageLifecycle,
  fromPage,
  hostOf,
  meetingsOf,
  navigation,
  originOf,
  pageMessage,
  permissionText,
  postScript,
  runCommand,
  videoOptions,
  widgetSections,
} from '../widgets';

const widget = (id: string, meeting: Widget['meeting'] = null): Widget => ({
  id,
  name: id,
  kind: meeting === null ? 'm.custom' : 'jitsi',
  url: `https://example.org/${id}`,
  host: 'example.org',
  creator: null,
  can_remove: false,
  meeting,
});

/** The run's bridge token in these tests. */
const TOKEN = 'token-1';

/** A camera stream, as a page that may use one gets it. */
const STREAM = 'stream';

/** What the web view's scripts use of a page's window. */
interface FakeWindow {
  location: { origin: string };
  document: { readyState: string };
  postMessage: (data: Json, origin?: string) => void;
  addEventListener: (type: string, listener: () => void) => void;
  ReactNativeWebView: { postMessage: (json: string) => void };
  navigator: {
    mediaDevices: { getUserMedia: () => Promise<string> };
    geolocation: {
      getCurrentPosition: (success: () => void, failure: (error: { code: number }) => void) => void;
      watchPosition: (success: () => void, failure: (error: { code: number }) => void) => number;
    };
  };
}

/** The page's origin in these tests. */
const ORIGIN = 'https://w.example';

/**
 * A page's main frame at `origin`, as far as the web view's scripts use it: what is delivered to
 * it with `postMessage` (`delivered`) and for which origin (`targets`), what the bridge hands the
 * app (`handed`), its camera, and its `load` event (`load`).
 */
function fakePage(origin = ORIGIN, readyState = 'loading') {
  const handed: string[] = [];
  const delivered: Json[] = [];
  const targets: Array<string | undefined> = [];
  const loads: Array<() => void> = [];
  const getUserMedia = () => Promise.resolve(STREAM);

  const page: FakeWindow = {
    location: { origin },
    document: { readyState },
    addEventListener: (type, listener) => {
      if (type === 'load') loads.push(listener);
    },
    postMessage: (data, origin) => {
      delivered.push(data);
      targets.push(origin);
    },
    ReactNativeWebView: { postMessage: (json) => handed.push(json) },
    navigator: {
      mediaDevices: { getUserMedia },
      // Where the user is, as a page that may know gets it.
      geolocation: { getCurrentPosition: (success) => success(), watchPosition: () => 1 },
    },
  };

  // The page is its own top frame.
  Object.assign(page, { top: page });

  // The scripts run in the page as the web view runs them, with `window` as the page's.
  const run = (script: string) => new Function('window', script)(page);

  const load = () => {
    for (const listener of loads) listener();
  };

  return { page, handed, delivered, targets, run, load };
}

/** A widget asking the app (the widget API). */
const asked = { api: 'fromWidget', action: 'capabilities', requestId: '1', data: {} };

/** A widget answering the app. */
const answered = { ...asked, api: 'toWidget', response: { capabilities: [] } };

/** The engine asking the widget, posted into the page. */
const driverAsks = { api: 'toWidget', action: 'capabilities', requestId: '2', data: {} };

/** The engine answering the widget, posted into the page. */
const driverAnswers = { ...asked, response: {} };

/** An integration manager asking the app. */
const manager = { action: 'invite', room_id: '!r:x', user_id: '@bot:x', requestId: '3' };

/** The app's answer to an integration manager (Scalar's), which may be `null`. */
const managerAnswered = { ...manager, response: null };

/** What the bridge script hands the app for `message`. */
const envelope = (message: Json, token = TOKEN) => JSON.stringify({ token, message });

/** Whether the app passes `message` from a page with `bridge` on to the engine. */
const passed = (bridge: WidgetBridge, message: Json) =>
  pageMessage(bridge, TOKEN, envelope(message)) !== null;

describe('runCommand', () => {
  it('should run one of the room’s widgets in the app', () => {
    expect(runCommand(ADA, '!r:x', { type: 'widget', widgetId: 'w1' })).toEqual({
      type: 'RunWidget',
      data: { account: ADA, room_id: '!r:x', widget_id: 'w1', browser: false },
    });
  });

  it('should start a video meeting in the app', () => {
    expect(runCommand(ADA, '!r:x', { type: 'meeting', video: true })).toEqual({
      type: 'StartMeeting',
      data: { account: ADA, room_id: '!r:x', video: true, browser: false },
    });
  });

  it('should open the integration manager for the room', () => {
    expect(runCommand(ADA, '!r:x', { type: 'integrations' })).toEqual({
      type: 'OpenIntegrations',
      data: { account: ADA, room_id: '!r:x' },
    });
  });
});

describe('meetingsOf', () => {
  it('should keep only the widgets holding a meeting', () => {
    const widgets = {
      widgets: [widget('board'), widget('standup', 'Jitsi'), widget('class', 'BigBlueButton')],
      can_add: false,
    };

    expect(meetingsOf(widgets).map((w) => w.id)).toEqual(['standup', 'class']);
  });

  it('should find no meetings while the room’s widgets are unknown', () => {
    expect(meetingsOf(undefined)).toEqual([]);
  });
});

describe('videoOptions', () => {
  const withCalls = { ...roomSummary('!r:x'), call: { via: 'Rtc' as const, participants: [] } };
  const standup = widget('standup', 'Jitsi');

  it('should offer the video call first where calls are available', () => {
    expect(videoOptions({ ...withCalls, can_start_meeting: true }, [standup])).toEqual([
      { kind: 'call' },
      { kind: 'meeting', widgetId: 'standup', name: 'standup' },
      { kind: 'start' },
    ]);
  });

  it('should join the pinned meeting first where there are no calls', () => {
    expect(videoOptions(roomSummary('!r:x'), [standup])[0]).toEqual({
      kind: 'meeting',
      widgetId: 'standup',
      name: 'standup',
    });
  });

  it('should start a meeting where there are no calls and none is pinned', () => {
    expect(videoOptions({ ...roomSummary('!r:x'), can_start_meeting: true }, [])).toEqual([
      { kind: 'start' },
    ]);
  });

  it('should offer nothing where there are no calls and no meetings', () => {
    expect(videoOptions(roomSummary('!r:x'), [])).toEqual([]);
  });

  it('should offer the meetings only while a call is under way', () => {
    expect(videoOptions({ ...withCalls, can_start_meeting: true }, [standup], true)).toEqual([
      { kind: 'meeting', widgetId: 'standup', name: 'standup' },
      { kind: 'start' },
    ]);
  });

  it('should offer nothing during a call where there are no meetings', () => {
    expect(videoOptions(withCalls, [], true)).toEqual([]);
  });

  it('should offer nothing in a room it doesn’t know', () => {
    expect(videoOptions(undefined, [standup])).toEqual([]);
  });
});

describe('widgetSections', () => {
  it('should list a room’s meetings apart from its other widgets, in order', () => {
    const widgets = {
      widgets: [
        widget('board'),
        widget('standup', 'Jitsi'),
        widget('grafana'),
        widget('class', 'BigBlueButton'),
      ],
      can_add: false,
    };

    const { meetings, others } = widgetSections(widgets);
    expect([meetings.map((w) => w.id), others.map((w) => w.id)]).toEqual([
      ['standup', 'class'],
      ['board', 'grafana'],
    ]);
  });

  it('should list nothing while the room’s widgets are unknown', () => {
    expect(widgetSections(undefined)).toEqual({ meetings: [], others: [] });
  });
});

describe('permissionText', () => {
  const permission = (kind: WidgetPermission['kind'], subject: string | null = null) => ({
    capability: `cap.${kind}`,
    kind,
    subject,
  });

  it('should say which events a widget would send as the user', () => {
    expect(permissionText(permission('SendEvents', 'm.room.message'))).toBe(
      'Send m.room.message events as you',
    );
  });

  it('should say which state a widget would see', () => {
    expect(permissionText(permission('ReadState', 'm.room.member'))).toBe(
      "See the room's m.room.member state",
    );
  });

  it('should say a widget wants the files the user’s account can open', () => {
    expect(permissionText(permission('DownloadFiles'))).toBe(
      'Download files your account can open',
    );
  });

  it('should say a widget wants to run only inside the app', () => {
    expect(permissionText(permission('StayInApp'))).toBe('Run only inside Better Matrix');
  });

  it('should name the server a widget wants to confirm the user’s identity to', () => {
    expect(permissionText(permission('Identity', 'https://jitsi.example'))).toBe(
      'Confirm your Matrix ID to https://jitsi.example',
    );
  });

  it('should say a widget wants to confirm who the user is when the server is unnamed', () => {
    expect(permissionText(permission('Identity'))).toBe(
      "Confirm your Matrix ID to the widget's server",
    );
  });

  it('should show a permission it doesn’t know by its capability', () => {
    expect(permissionText(permission('Other'))).toBe('cap.Other');
  });

  it('should show a permission missing its subject by its capability', () => {
    expect(permissionText(permission('SendEvents'))).toBe('cap.SendEvents');
  });
});

describe('pageMessage', () => {
  it('should pass a widget’s request to the engine', () =>
    expect(passed('WidgetApi', asked)).toBe(true));
  it('should pass a widget’s answer to the engine', () =>
    expect(passed('WidgetApi', answered)).toBe(true));
  it('should keep the engine’s own request to the widget in the page', () =>
    expect(passed('WidgetApi', driverAsks)).toBe(false));
  it('should keep the engine’s own answer to the widget in the page', () =>
    expect(passed('WidgetApi', driverAnswers)).toBe(false));
  it('should pass an integration manager’s request', () =>
    expect(passed('Integrations', manager)).toBe(true));
  it('should keep the widget API out of the integration manager’s bridge', () =>
    expect(passed('Integrations', { ...manager, api: 'fromWidget' })).toBe(false));
  it('should keep an answer out of the integration manager’s bridge', () =>
    expect(passed('Integrations', { ...manager, response: {} })).toBe(false));
  it('should keep a null answer out of the integration manager’s bridge', () =>
    expect(passed('Integrations', managerAnswered)).toBe(false));
  it('should pass nothing from a page without a bridge', () =>
    expect(passed('None', asked)).toBe(false));
  it('should pass nothing that isn’t JSON', () =>
    expect(pageMessage('WidgetApi', TOKEN, '{"token":')).toBeNull());
  it('should pass no JSON array', () => expect(passed('Integrations', [manager])).toBe(false));
  it('should pass no message whose api isn’t a name', () =>
    expect(passed('WidgetApi', { ...asked, api: 7 })).toBe(false));
  it('should pass nothing without the run’s token', () =>
    expect(pageMessage('WidgetApi', TOKEN, envelope(asked, 'guessed'))).toBeNull());
  it('should pass nothing posted outside an envelope', () =>
    expect(pageMessage('WidgetApi', TOKEN, JSON.stringify(asked))).toBeNull());

  it('should hand the engine the message as it was posted', () => {
    expect(JSON.parse(pageMessage('WidgetApi', TOKEN, envelope(asked)) ?? 'null')).toEqual(asked);
  });
});

describe('origins', () => {
  it('should read an address’s origin without its path, user or default port', () =>
    expect(originOf('HTTPS://ada@Meet.Example.org:443/room?x=1#y')).toBe(
      'https://meet.example.org',
    ));
  it('should keep a port that isn’t the default', () =>
    expect(originOf('https://meet.example.org:8443/x')).toBe('https://meet.example.org:8443'));
  it('should find no origin in an address without a host', () =>
    expect(originOf('about:blank')).toBeNull());
  it('should take a message from the page’s own origin', () =>
    expect(fromPage('https://w.example/board?id=1', 'https://w.example')).toBe(true));
  it('should refuse a message from another origin', () =>
    expect(fromPage('https://w.example/board', 'https://ads.example/frame')).toBe(false));
});

describe('hosts and documents', () => {
  it('should read an address’s host without its port', () =>
    expect(hostOf('https://BBB.example:8443/join')).toBe('bbb.example'));
  it('should find no host in an address without one', () =>
    expect(hostOf('about:blank')).toBeNull());
});

describe('navigation', () => {
  const page: WidgetPage = { url: 'https://w.example/board', title: 'Board', bridge: 'WidgetApi' };
  const meeting: WidgetPage = { ...page, url: 'https://bbb.example/join', bridge: 'None' };

  it('should load the page’s own origin', () =>
    expect(navigation(page, 'https://w.example/next', true)).toBe('load'));
  it('should open another site in the browser for a page with a bridge', () =>
    expect(navigation(page, 'https://evil.example/', true)).toBe('browser'));
  it('should let a meeting go to another site in the app', () =>
    expect(navigation(meeting, 'https://bbb-2.example/client', true)).toBe('load'));
  it('should refuse a page that isn’t on the web', () =>
    expect(navigation(page, 'data:text/html,hi', true)).toBe('refuse'));
  it('should refuse a meeting a page that isn’t on the web', () =>
    expect(navigation(meeting, 'intent://x#Intent;end', true)).toBe('refuse'));
  it('should take a navigation that doesn’t say its frame for the top frame', () =>
    expect(navigation(page, 'https://evil.example/', undefined)).toBe('browser'));
  it('should let frames inside the page load what they like', () =>
    expect(navigation(page, 'https://player.example/embed', false)).toBe('load'));
});

describe('bridgeScript', () => {
  it('should hand the app nothing from a meeting', () => {
    const { page, handed, run } = fakePage();
    run(bridgeScript('None', TOKEN));
    page.postMessage(asked, '*');
    expect(handed).toEqual([]);
  });

  it.each(['None', 'WidgetApi'] as const)(
    'should keep the user’s location from a %s page',
    async (bridge) => {
      const { page, run } = fakePage();
      run(bridgeScript(bridge, TOKEN));

      const failed = await new Promise<number>((resolve) =>
        page.navigator.geolocation.getCurrentPosition(
          () => resolve(0),
          (error) => resolve(error.code),
        ),
      );

      expect(failed).toBe(1);
    },
  );

  it.each(['None', 'WidgetApi'] as const)(
    'should do nothing when a %s page’s script is installed again',
    (bridge) => {
      const { page, handed, delivered, run } = fakePage(ORIGIN, 'complete');
      run(bridgeScript(bridge, TOKEN));
      const reports = handed.length;
      run(bridgeScript(bridge, TOKEN));
      page.postMessage(driverAsks, '*');
      expect([handed.length, delivered]).toEqual([reports, [driverAsks]]);
    },
  );

  it('should do nothing in a frame inside the page', () => {
    const { page, handed, delivered, run } = fakePage(ORIGIN, 'complete');
    Object.assign(page, { top: {} });
    run(bridgeScript('WidgetApi', TOKEN));
    page.postMessage(manager, '*');
    expect([handed, delivered]).toEqual([[], [manager]]);
  });

  it('should keep its install flag out of the page’s own properties', () => {
    const { page, run } = fakePage();
    run(bridgeScript('WidgetApi', TOKEN));
    expect(Object.keys(page).some((key) => key.includes(TOKEN))).toBe(false);
  });

  it('should keep the user’s location from Geolocation’s own methods too', async () => {
    const { page, run } = fakePage();

    class FakeGeolocation {
      getCurrentPosition(success: () => void, _failure: (error: { code: number }) => void) {
        success();
      }
    }

    Object.assign(page, { Geolocation: FakeGeolocation });
    const geolocation = new FakeGeolocation();
    run(bridgeScript('None', TOKEN));

    const failed = await new Promise<number>((resolve) =>
      FakeGeolocation.prototype.getCurrentPosition.call(
        geolocation,
        () => resolve(0),
        (error) => resolve(error.code),
      ),
    );

    expect(failed).toBe(1);
  });

  it('should report a document that loaded before the script ran as loaded at once', () => {
    const { handed, run } = fakePage(ORIGIN, 'complete');
    run(bridgeScript('WidgetApi', TOKEN));
    expect(handed.map((json) => pageLifecycle(TOKEN, json))).toEqual(['loading', 'loaded']);
  });

  it.each(['WidgetApi', 'Integrations'] as const)(
    'should hand the app from a %s page exactly what the app passes on',
    (bridge) => {
      const { page, handed, run } = fakePage();
      run(bridgeScript(bridge, TOKEN));
      const messages = [asked, answered, driverAsks, driverAnswers, manager, managerAnswered];

      handed.length = 0;

      for (const message of messages) page.postMessage(message, '*');

      expect(handed).toEqual(
        messages.flatMap((message) => (passed(bridge, message) ? [envelope(message)] : [])),
      );
    },
  );

  it('should not deliver to the page what it hands the app', () => {
    const { page, delivered, run } = fakePage();
    run(bridgeScript('Integrations', TOKEN));
    page.postMessage(manager, '*');
    expect(delivered).toEqual([]);
  });

  it('should deliver to the page what it doesn’t hand the app', () => {
    const { page, delivered, run } = fakePage();
    run(bridgeScript('WidgetApi', TOKEN));
    page.postMessage(driverAsks, '*');
    expect(delivered).toEqual([driverAsks]);
  });

  it('should hand the app nothing that isn’t an object', () => {
    const { page, handed, run } = fakePage();
    run(bridgeScript('Integrations', TOKEN));
    handed.length = 0;
    page.postMessage('{"action":"invite"}', '*');
    page.postMessage([manager], '*');
    expect(handed).toEqual([]);
  });

  it('should report its document loading as it runs, and loaded on its load event', () => {
    const { handed, run, load } = fakePage();
    run(bridgeScript('WidgetApi', TOKEN));
    load();
    expect(handed.map((json) => pageLifecycle(TOKEN, json))).toEqual(['loading', 'loaded']);
  });

  it('should refuse the camera to a widget that isn’t a meeting', async () => {
    const { page, run } = fakePage();
    run(bridgeScript('WidgetApi', TOKEN));
    await expect(page.navigator.mediaDevices.getUserMedia()).rejects.toMatchObject({
      name: 'NotAllowedError',
    });
  });
});

describe('postScript', () => {
  /** The script for `message` to the page's origin; these tests' messages are objects. */
  const script = (message: Json, origin = ORIGIN) =>
    postScript(JSON.stringify(message), origin) ?? '';

  it('should check the document’s origin before the message is there, as a literal', () => {
    expect(script(driverAsks)).toBe(
      `(function () {
  if (window.location.origin !== "https://w.example") return;
  var message = ${JSON.stringify(driverAsks)};
  window.postMessage(message, "https://w.example");
})();
true;`,
    );
  });

  it('should deliver the engine’s message to the page as an object', () => {
    const { delivered, run } = fakePage();
    run(script(driverAsks));
    expect(delivered).toEqual([driverAsks]);
  });

  it('should deliver the engine’s message for the page’s own origin only', () => {
    const { targets, run } = fakePage();
    run(script(driverAsks));
    expect(targets).toEqual([ORIGIN]);
  });

  it('should deliver nothing to a document of another origin', () => {
    const { delivered, run } = fakePage('https://evil.example');
    run(script(driverAsks));
    expect(delivered).toEqual([]);
  });

  it('should keep an origin inside its string', () => {
    const tricky = 'x\'); window.evil = 1; (\'"); window.evil = 1; ("';
    const { targets, page, run } = fakePage(tricky);
    run(script(driverAsks, tricky));
    expect([targets, 'evil' in page]).toEqual([[tricky], false]);
  });

  it('should deliver every engine message through the bridge to the page', () => {
    const { delivered, handed, run } = fakePage();
    run(bridgeScript('WidgetApi', TOKEN));
    handed.length = 0;

    for (const message of [driverAsks, driverAnswers]) run(script(message));

    expect([delivered, handed]).toEqual([[driverAsks, driverAnswers], []]);
  });

  it('should keep quotes, backslashes, line separators and tags inside the message', () => {
    const tricky = {
      text: '"); window.evil = 1; ("\\\n\u2028\u2029</script><script>alert(1)</script>',
    };

    const { page, delivered, run } = fakePage();
    run(script(tricky));
    expect([delivered, 'evil' in page]).toEqual([[tricky], false]);
  });

  it('should keep a __proto__ key a key', () => {
    const { delivered, run } = fakePage();
    run(postScript('{"data":{"__proto__":{"polluted":true}}}', ORIGIN) ?? '');
    expect(JSON.stringify(delivered)).toBe('[{"data":{"__proto__":{"polluted":true}}}]');
  });

  it('should refuse a message that isn’t JSON', () => {
    expect(postScript('not json', ORIGIN)).toBeNull();
  });

  it('should refuse a message that isn’t an object', () => {
    expect([postScript('[1]', ORIGIN), postScript('"text"', ORIGIN)]).toEqual([null, null]);
  });
});

describe('pageLifecycle', () => {
  it('should read a report with the run’s token', () =>
    expect(pageLifecycle(TOKEN, JSON.stringify({ token: TOKEN, lifecycle: 'loaded' }))).toBe(
      'loaded',
    ));
  it('should ignore a report without the run’s token', () =>
    expect(pageLifecycle(TOKEN, JSON.stringify({ token: 'x', lifecycle: 'loaded' }))).toBeNull());
  it('should ignore a message that isn’t a report', () =>
    expect(pageLifecycle(TOKEN, envelope(asked))).toBeNull());
});

describe('captureGrant', () => {
  it('should let iOS ask for a meeting site by site', () =>
    expect(captureGrant('ios', 'bbb.example')).toEqual({ grantType: 'prompt', host: undefined }));
  it('should bind Android’s web view to the meeting’s approved host', () =>
    expect(captureGrant('android', 'bbb.example')).toEqual({
      grantType: 'grantIfSameHostElseDeny',
      host: 'bbb.example',
    }));
  it('should deny a widget that isn’t a meeting', () =>
    expect([captureGrant('ios', null), captureGrant('android', null)]).toEqual([
      { grantType: 'deny', host: undefined },
      { grantType: 'deny', host: undefined },
    ]));
});

describe('page failures', () => {
  it('should say a load failure by its domain and code only', () =>
    expect(loadFailure(-1009, 'NSURLErrorDomain')).toBe('NSURLErrorDomain -1009'));
  it('should say a load failure by its code where there is no domain', () =>
    expect(loadFailure(-2, undefined)).toBe('-2'));
  it('should say an HTTP failure by its status only', () =>
    expect(httpFailure(404)).toBe('HTTP 404'));
});
