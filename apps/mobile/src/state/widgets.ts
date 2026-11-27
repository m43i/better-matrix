// Widgets run inside the app (ADR 0010): what to run, the commands that run it, the room's
// meetings, the words for a widget's permissions, and the bridge between a widget's page in the
// web view and the engine. The page's messages pass to the engine only as its bridge says
// (`WidgetBridge`), from the page's own main frame (the run's token, the page's origin); the
// engine's are posted into the page as objects. A page with a bridge stays on its origin, and
// only a meeting (no bridge) may use the camera and microphone. Pure, so the web view's scripts are tested here.

import type {
  AccountId,
  Command,
  RoomSummary,
  Widget,
  WidgetAccess,
  WidgetUrlField,
  WidgetBridge,
  WidgetPage,
  WidgetPermission,
  Widgets,
} from '@better-matrix/matrix-native/types';
import { z } from 'zod';

import { t } from '../i18n';

/** What the widget screen runs: one of the room's widgets, a new meeting, or integrations. */
export type WidgetTarget =
  | { type: 'widget' | 'permissions'; widgetId: string }
  | { type: 'meeting'; video: boolean }
  | { type: 'integrations' };

/** The command that runs `target` in the app's web view. */
export function runCommand(account: AccountId, roomId: string, target: WidgetTarget): Command {
  switch (target.type) {
    case 'widget':
      return {
        type: 'RunWidget',
        data: { account, room_id: roomId, widget_id: target.widgetId, browser: false },
      };
    case 'permissions':
      return {
        type: 'ManageWidgetPermissions',
        data: { account, room_id: roomId, widget_id: target.widgetId },
      };
    case 'meeting':
      return {
        type: 'StartMeeting',
        data: { account, room_id: roomId, video: target.video, browser: false },
      };
    case 'integrations':
      return { type: 'OpenIntegrations', data: { account, room_id: roomId } };
  }
}

/** The video meetings among a room's widgets, which the room offers to join. */
export function meetingsOf(widgets: Widgets | undefined): Widget[] {
  return widgets?.widgets.filter((widget) => widget.meeting !== null) ?? [];
}

/** What the room header's video button can do, as the desktop's offers it. */
export type VideoOption =
  /** The room's own video call. */
  | { kind: 'call' }
  /** Join a meeting pinned to the room. */
  | { kind: 'meeting'; widgetId: string; name: string }
  /** Start a meeting in the room. */
  | { kind: 'start' };

/**
 * The video button's options, the default first: the video call where calls are available
 * (`call.via`) and none is under way (`callUnderWay`: the header shows Join or Return instead),
 * joining each pinned meeting, and starting one where the room can (`can_start_meeting`). None:
 * no video button.
 */
export function videoOptions(
  room: RoomSummary | undefined,
  meetings: readonly Widget[],
  callUnderWay = false,
): VideoOption[] {
  if (!room) return [];
  const options: VideoOption[] = room.call.via === null || callUnderWay ? [] : [{ kind: 'call' }];

  for (const meeting of meetings)
    options.push({ kind: 'meeting', widgetId: meeting.id, name: meeting.name });

  if (room.can_start_meeting) options.push({ kind: 'start' });

  return options;
}

/** A room's widgets as the widgets screen lists them: its meetings first, then the rest. */
export interface WidgetSections {
  meetings: Widget[];
  others: Widget[];
}

/** A room's widgets split into `WidgetSections`, each in the room's order. */
export function widgetSections(widgets: Widgets | undefined): WidgetSections {
  const meetings: Widget[] = [];
  const others: Widget[] = [];

  for (const widget of widgets?.widgets ?? [])
    (widget.meeting === null ? others : meetings).push(widget);

  return { meetings, others };
}

const urlFields = {
  UserId: 'widgets.field.user_id',
  RoomId: 'widgets.field.room_id',
  DisplayName: 'widgets.field.display_name',
  AvatarUrl: 'widgets.field.avatar_url',
  DeviceId: 'widgets.field.device_id',
  Homeserver: 'widgets.field.homeserver',
} satisfies Record<WidgetUrlField, Parameters<typeof t>[0]>;

export function accessContext(access: WidgetAccess): string {
  return t('widgets.access_context', {
    site: access.origin,
    room: access.room,
    user: access.user_id,
  });
}

export function accessText(access: WidgetAccess) {
  const { kind, name } = access;

  switch (kind.type) {
    case 'Open':
      return {
        title: t('widgets.open_title', { widget: name }),
        body: kind.data.length
          ? t('widgets.open_shares', {
              fields: kind.data.map((field) => t(urlFields[field])).join(', '),
            })
          : t('widgets.open_site'),
        action: t('common.continue'),
      };
    case 'Manager':
      return {
        title: t('widgets.integrations_open'),
        body: t(kind.data.hosted ? 'widgets.manager_hosted' : 'widgets.manager_configured'),
        action: t('common.continue'),
      };
    case 'Saved':
      return {
        title: t('widgets.permissions_title', { widget: name }),
        body: kind.data.length
          ? kind.data.map(permissionText).join('\n')
          : t('widgets.no_saved_permissions'),
        action: t('widgets.reset_permissions'),
      };
  }
}

/** What a widget permission allows, in words. */
export function permissionText(permission: WidgetPermission): string {
  const subject = permission.subject;

  switch (permission.kind) {
    case 'ReadEvents':
      return subject === null
        ? permission.capability
        : t('mobile.widgets.permission_read_events', { subject });
    case 'SendEvents':
      return subject === null
        ? permission.capability
        : t('mobile.widgets.permission_send_events', { subject });
    case 'ReadState':
      return subject === null
        ? permission.capability
        : t('mobile.widgets.permission_read_state', { subject });
    case 'SendState':
      return subject === null
        ? permission.capability
        : t('mobile.widgets.permission_send_state', { subject });
    case 'ReadToDevice':
      return subject === null
        ? permission.capability
        : t('mobile.widgets.permission_read_to_device', { subject });
    case 'SendToDevice':
      return subject === null
        ? permission.capability
        : t('mobile.widgets.permission_send_to_device', { subject });
    case 'DelayedEvents':
      return t('mobile.widgets.permission_delayed_events');
    case 'DownloadFiles':
      return t('mobile.widgets.permission_download_files');
    case 'CallServices':
      return t('mobile.widgets.permission_call_services');
    case 'StayInApp':
      return t('mobile.widgets.permission_stay_in_app');
    case 'Identity':
      // The subject is the server that will learn who the user is.
      return subject === null
        ? t('mobile.widgets.permission.identity')
        : t('mobile.widgets.permission.identity_to', { subject });
    case 'Other':
      return permission.capability;
  }
}

/**
 * What the bridge reads of a message the page posted: a JSON object, by the fields that decide
 * where it goes (an `api` that isn't a string isn't the widget API's, nor anyone's).
 */
const pageMessageSchema = z.looseObject({
  api: z.string().optional(),
  action: z.json().optional(),
  response: z.json().optional(),
});

export type PageMessage = z.infer<typeof pageMessageSchema>;

/**
 * Whether a message the page posted passes to the engine under `bridge`. A message "holds a
 * response" when it has the key, even as `null`.
 */
export function passes(bridge: WidgetBridge, message: PageMessage): boolean {
  const answers = 'response' in message;

  switch (bridge) {
    case 'None':
      return false;
    case 'WidgetApi':
      return (message.api === 'fromWidget' && !answers) || (message.api === 'toWidget' && answers);
    case 'Integrations':
      return message.action !== undefined && message.api === undefined && !answers;
  }
}

/** What the bridge script hands the app: the run's token and the page's message. */
const envelopeSchema = z.object({ token: z.string(), message: pageMessageSchema });

/** The bridge script's envelope, read from JSON: `null` unless it is one. */
const pageJson = z
  .string()
  .transform((json, context) => {
    try {
      return JSON.parse(json);
    } catch {
      context.addIssue({ code: 'custom', message: 'Invalid page message JSON' });

      return z.NEVER;
    }
  })
  .pipe(envelopeSchema)
  .nullable()
  .catch(null).parse;

/** The bridge script's report of its document: started (`loading`) or loaded (`loaded`). */
export type Lifecycle = 'loading' | 'loaded';

const lifecycleSchema = z.object({
  token: z.string(),
  lifecycle: z.enum(['loading', 'loaded']),
});

/** A lifecycle report, read from JSON: `null` unless it is one. */
const lifecycleJson = z
  .string()
  .transform((json, context) => {
    try {
      return JSON.parse(json);
    } catch {
      context.addIssue({ code: 'custom', message: 'Invalid page message JSON' });

      return z.NEVER;
    }
  })
  .pipe(lifecycleSchema)
  .nullable()
  .catch(null).parse;

/**
 * The page's document as its bridge script reports it, with the run's `token`: started when the
 * script ran (a new document; a History API change runs no script), loaded on its `load` event.
 * `null` for anything else.
 */
export function pageLifecycle(token: string, raw: string): Lifecycle | null {
  const report = lifecycleJson(raw);

  return report !== null && report.token === token ? report.lifecycle : null;
}

/**
 * The page's message as the engine takes it (`Answer::WidgetMessage`): the message in an
 * envelope carrying the run's `token` (only the main frame's bridge script knows it: Android
 * hands `ReactNativeWebView` to frames inside the page too), a JSON object that passes `bridge`;
 * else `null`.
 */
export function pageMessage(bridge: WidgetBridge, token: string, raw: string): string | null {
  const envelope = pageJson(raw);

  if (envelope === null || envelope.token !== token || !passes(bridge, envelope.message))
    return null;

  return JSON.stringify(envelope.message);
}

/**
 * An address's origin (scheme, host and port, the default port left out), as a page's is
 * compared: `null` for one with no host (`about:`, `data:`).
 */
export function originOf(url: string): string | null {
  const parts = /^([a-z][a-z0-9+.-]*):\/\/(?:[^@/?#]*@)?([^/?#]+)/i.exec(url);
  const scheme = parts?.[1]?.toLowerCase();
  const host = parts?.[2]?.toLowerCase();

  if (scheme === undefined || host === undefined) return null;
  const port = scheme === 'https' ? ':443' : scheme === 'http' ? ':80' : null;

  return `${scheme}://${port !== null && host.endsWith(port) ? host.slice(0, -port.length) : host}`;
}

/** An address's host, lower case, without its port: `null` for one with no host. */
export function hostOf(url: string): string | null {
  const origin = originOf(url);

  return origin === null ? null : origin.replace(/^[^:]+:\/\//, '').replace(/:\d+$/, '');
}

/** The web view's camera and microphone settings for a page. */
export interface Capture {
  /** `mediaCapturePermissionGrantType`: one of its five values (Android's view parses no other). */
  grantType: 'prompt' | 'grantIfSameHostElseDeny' | 'deny';
  /** The patched Android web view's `mediaCaptureHost`: the only host that may use them. */
  host: string | undefined;
}

/**
 * Who may use the camera and microphone in a widget's page, bound to the meeting's approved
 * `host` natively: on iOS a meeting gets `prompt` (WebKit asks for each site itself); on Android
 * `grantIfSameHostElseDeny` with the host, which the patched web view grants to pages on that
 * host only (patches/react-native-webview@14.0.1.patch). A widget that isn't a meeting (`host`
 * null) gets `deny`.
 */
export function captureGrant(platform: 'ios' | 'android', host: string | null): Capture {
  if (host === null) return { grantType: 'deny', host: undefined };

  return platform === 'ios'
    ? { grantType: 'prompt', host: undefined }
    : { grantType: 'grantIfSameHostElseDeny', host };
}

/**
 * Why a page failed to load, as the screen says it and the log keeps it: the error's code and
 * domain only, never its description or address, which may carry a token (a Jitsi JWT, a
 * Scalar token).
 */
export function loadFailure(code: number, domain: string | undefined): string {
  return domain ? `${domain} ${code}` : `${code}`;
}

/** Why a page failed to load over HTTP: its status only (see `loadFailure`). */
export function httpFailure(status: number): string {
  return `HTTP ${status}`;
}

/** Whether a message from `from` (the frame that posted it) comes from the page at `pageUrl`. */
export function fromPage(pageUrl: string, from: string): boolean {
  const origin = originOf(pageUrl);

  return origin !== null && originOf(from) === origin;
}

/** What the web view does with a navigation the page starts. */
export type Navigation = 'load' | 'browser' | 'refuse';

/**
 * A navigation in a widget's page: frames inside it go where they like. A page with a bridge
 * stays on its origin, since the bridge trusts it: a web page elsewhere opens in the system's
 * browser. A meeting has no bridge and may go anywhere on the web (a BigBlueButton join link
 * redirects to its server). Anything not on the web (another scheme, `data:`, `about:`) is
 * refused in the page itself.
 */
export function navigation(
  page: WidgetPage,
  url: string,
  /** Whether the top frame navigates; Android's web view leaves it out, so unknown is the top. */
  topFrame: boolean | undefined,
): Navigation {
  if (topFrame === false || fromPage(page.url, url)) return 'load';

  if (!/^https?:\/\//i.test(url)) return 'refuse';

  return page.bridge === 'None' ? 'load' : 'browser';
}

/** The outgoing test `passes` makes, as the page's own JavaScript over `data`. */
const PAGE_TEST: Record<Exclude<WidgetBridge, 'None'>, string> = {
  WidgetApi:
    "(data.api === 'fromWidget' && !('response' in data)) || " +
    "(data.api === 'toWidget' && 'response' in data)",
  Integrations: "data.action !== undefined && data.api === undefined && !('response' in data)",
};

/** A string as a JavaScript literal that nothing in it can end, inside a script. */
function literal(text: string): string {
  return JSON.stringify(text)
    .replace(/\u2028/g, '\\u2028')
    .replace(/\u2029/g, '\\u2029')
    .replace(/</g, '\\u003c');
}

/**
 * Where the user is stays with the app: `getCurrentPosition` and `watchPosition` fail with
 * `PERMISSION_DENIED`, on `Geolocation.prototype` as well as `navigator.geolocation`, so the page
 * can't reach the originals (Android's web view has `geolocationEnabled` off as well; on iOS this
 * script is the guard, and frames inside the page, which it doesn't reach, still meet WebKit's
 * own prompt per site).
 */
const NO_LOCATION = `
  var denied = function (failure) {
    if (!failure) return;
    setTimeout(function () {
      failure({ code: 1, message: 'User denied Geolocation', PERMISSION_DENIED: 1, POSITION_UNAVAILABLE: 2, TIMEOUT: 3 });
    }, 0);
  };
  var refuseLocation = function (target) {
    if (!target) return;
    try {
      target.getCurrentPosition = function (success, failure) {
        denied(failure);
      };
      target.watchPosition = function (success, failure) {
        denied(failure);
        return 0;
      };
    } catch (error) {}
  };
  refuseLocation(window.Geolocation && window.Geolocation.prototype);
  refuseLocation(window.navigator && window.navigator.geolocation);`;

/**
 * The start of every run's script: it runs in the top frame only (Android's document-start
 * scripts reach every frame) and installs once per document. A second install (the app's
 * fallback, `WidgetRun.pageEnded`) finds the run's flag, a property named with the run's token,
 * which the page can't name ahead of it, and does nothing.
 */
function installOnce(token: string): string {
  return `
  if (window.top !== window) return;
  var installed = ${literal(`__betterMatrix_${token}`)};
  if (Object.prototype.hasOwnProperty.call(window, installed)) return;
  Object.defineProperty(window, installed, { value: true, enumerable: false });`;
}

/**
 * The script the web view runs in the page's main frame before the page's own
 * (`injectedJavaScriptBeforeContentLoaded`), for every widget:
 *
 * - It installs once per document (`installOnce`): the app may inject it again when Android ran
 *   none before the page loaded (`WidgetRun.pageEnded`).
 * - It keeps the user's location from the page (`NO_LOCATION`).
 *
 * And for a widget with a bridge (a meeting has none):
 *
 * - It reports its document to the app (`pageLifecycle`): `loading` as it runs, at the start of
 *   each new document, and `loaded` at once if the document has loaded already (Android may run
 *   it after the page's `load`), else on the window's `load` event.
 * - It takes over `window.postMessage`: a message the bridge passes (the page asking or
 *   answering the app, posting to itself as its parent) goes to the app in an envelope with the
 *   run's `token`, and is not delivered to the page, which would otherwise read its own request
 *   as an answer; anything else (the engine's messages among them) is delivered as before.
 * - It refuses the camera, microphone and screen (`getUserMedia`, `getDisplayMedia`): only a
 *   meeting may use them.
 */
export function bridgeScript(bridge: WidgetBridge, token: string): string {
  if (bridge === 'None')
    return `(function () {${installOnce(token)}${NO_LOCATION}
})();
true;`;

  return `(function () {${installOnce(token)}${NO_LOCATION}
  var token = ${literal(token)};
  var native = window.ReactNativeWebView;
  var deliver = window.postMessage;
  var report = function (lifecycle) {
    try {
      (native || window.ReactNativeWebView).postMessage(JSON.stringify({ token: token, lifecycle: lifecycle }));
    } catch (error) {}
  };
  report('loading');
  if (window.document && window.document.readyState === 'complete') report('loaded');
  else
    window.addEventListener('load', function () {
      report('loaded');
    });
  window.postMessage = function (data) {
    if (data && typeof data === 'object' && !Array.isArray(data) && (${PAGE_TEST[bridge]})) {
      try {
        (native || window.ReactNativeWebView).postMessage(JSON.stringify({ token: token, message: data }));
      } catch (error) {}
      return;
    }
    return deliver.apply(window, arguments);
  };
  var refuse = function () {
    var error;
    try {
      error = new window.DOMException('Not allowed in this widget', 'NotAllowedError');
    } catch (failed) {
      error = new Error('Not allowed in this widget');
      error.name = 'NotAllowedError';
    }
    return Promise.reject(error);
  };
  var navigator = window.navigator;
  var devices = navigator && navigator.mediaDevices;
  if (devices) {
    devices.getUserMedia = refuse;
    devices.getDisplayMedia = refuse;
  }
  if (navigator) {
    var legacy = function (constraints, success, failure) {
      refuse().catch(function (error) {
        if (failure) failure(error);
      });
    };
    navigator.getUserMedia = legacy;
    navigator.webkitGetUserMedia = legacy;
  }
})();
true;`;
}

/** What an engine message must be: a JSON object. */
const engineObject = z.record(z.string(), z.json());

/**
 * An engine message as compact JSON, when it is a JSON object; `null` else. Checked with
 * `engineObject`, but written from what `JSON.parse` read, keys and all (zod's copy leaves a
 * `__proto__` key out).
 */
function engineJson(message: string): string | null {
  let parsed: ReturnType<typeof JSON.parse>;

  try {
    parsed = JSON.parse(message);
  } catch {
    return null;
  }

  return engineObject.safeParse(parsed).success ? JSON.stringify(parsed) : null;
}

/**
 * Compact JSON (`JSON.stringify`'s) as a script's object literal: nothing in it can end the
 * script it stands in, and a `__proto__` key stays a key (a literal's would set the prototype),
 * written as a computed key: in compact JSON a key's quote follows `{` or `,`, which no quote
 * inside a string does (those are escaped).
 */
function scriptJson(json: string): string {
  return json
    .replace(/(?<=[{,])"__proto__":/g, '["__proto__"]:')
    .replace(/\u2028/g, '\\u2028')
    .replace(/\u2029/g, '\\u2029')
    .replace(/</g, '\\u003c');
}

/**
 * The script that posts an engine message (`Event::WidgetMessage`, JSON) into the page, as an
 * object, only if the document it runs in has the page's `origin`: it checks
 * `window.location.origin` (which a page can't forge) before the message exists in the document
 * at all, written as an object literal (no `JSON.parse` the page could have replaced), and posts
 * it for that origin only. The literal and the origin are escaped so nothing in them can end the
 * script. `null` for a message that isn't a JSON object. No engine message passes the bridge's
 * outgoing test, so each is delivered to the page.
 */
export function postScript(message: string, origin: string): string | null {
  const json = engineJson(message);

  if (json === null) return null;

  return `(function () {
  if (window.location.origin !== ${literal(origin)}) return;
  var message = ${scriptJson(json)};
  window.postMessage(message, ${literal(origin)});
})();
true;`;
}
