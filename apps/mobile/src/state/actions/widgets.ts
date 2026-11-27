// Running a widget in the app's web view (`RunWidget`, `StartMeeting`, `OpenIntegrations`): one
// `WidgetRun` per widget screen. It starts the command with `Store.run`, hears the run's own
// events (its page, the engine's messages for the page, a permission request, the integration
// manager's terms), answers them, and cancels the run when the screen goes. The page's main frame
// says when it has loaded (`WidgetLoaded`, which starts the engine's side) and its messages come
// with the run's `token` from the page's origin. Opening a widget in the browser instead is
// `openWidgetInBrowser`.

import type {
  AccountId,
  Answer,
  Policy,
  RequestId,
  WidgetAccess,
  WidgetPage,
  WidgetPermission,
} from '@better-matrix/matrix-native/types';

import { CommandFailed } from '../requests';
import type { Store } from '../store';
import {
  fromPage,
  hostOf,
  originOf,
  pageLifecycle,
  pageMessage,
  runCommand,
  type Lifecycle,
  type WidgetTarget,
} from '../widgets';
import { answerRequest } from './accounts';

/** What a widget screen shows. */
export type WidgetView =
  /** Waiting for the page (pinning a meeting, asking the integration manager). */
  | { type: 'starting' }
  | { type: 'access'; access: WidgetAccess }
  /** The integration manager's terms, to agree to before it opens. */
  | { type: 'terms'; policies: Policy[] }
  /**
   * The page, in the web view; `load` counts the engine's `WidgetPage`s, each a fresh load of
   * the page (the engine sends it again to load the page again).
   */
  | { type: 'page'; page: WidgetPage; load: number }
  /** The engine opens the widget in the browser instead (a page on plain HTTP): the screen goes. */
  | { type: 'browser'; url: string }
  /** The run is over (the widget or manager closed itself): the screen goes. */
  | { type: 'ended' }
  /** It failed, with why. */
  | { type: 'failed'; error: string };

/** A widget asking for permissions: the user grants some, all or none. */
export interface PermissionAsk {
  /** The run that asks. */
  request: RequestId;
  /** The question's number within the run, which the answer names. */
  question: number;
  widget: string;
  permissions: WidgetPermission[];
  context?: WidgetAccess;
}

export interface WidgetRunSnapshot {
  view: WidgetView;
  /** A permission request waiting for the user, over the page. */
  asking: PermissionAsk | null;
  /** The host a meeting may use the camera and microphone on; `null` for other widgets. */
  captureHost: string | null;
  /** A meeting that moved to another host: whether that host may use the camera, to ask once. */
  askingCapture: string | null;
}

/**
 * One run of a widget in the web view. Views read `snapshot()` and redraw on `subscribe`, as with
 * `QrLoginFlow`. `token` is the run's secret for the bridge script (`bridgeScript`): random, made
 * by the screen.
 *
 * The page's bridge script reports each new document (`loading`, then `loaded` on its `load`
 * event; a History API change runs no script), which the engine hears as `WidgetLoading` and
 * `WidgetLoaded`. The web view reports the main frame's address (`pageAt`), and the engine's
 * messages go into the page (`attach`) only while the document has loaded and that address has
 * exactly the page's origin, else they wait (and are dropped when the run ends); the posting
 * script checks the document's origin again itself (`postScript`). A meeting may use the camera
 * and microphone on its approved host (`captureHost`, bound natively: `captureGrant`), at first
 * its page's own; where `askHosts` (Android, whose web view grants by host; iOS's asks per site
 * itself), moving to another host asks the user once whether it may (`askingCapture`,
 * `answerCapture`). A question the engine withdraws (`WidgetPermissions` with nothing asked,
 * naming its number) closes without an answer. A page leaving its origin starts over. A widget
 * the engine opens in the browser instead (`OpenUrl`) leaves the screen (`browser`).
 */
export class WidgetRun {
  private current: WidgetRunSnapshot = {
    view: { type: 'starting' },
    asking: null,
    captureHost: null,
    askingCapture: null,
  };
  private request: RequestId | null = null;
  private access: WidgetAccess | undefined;
  private readonly listeners = new Set<() => void>();
  /** The engine's messages that came while no page was loaded, in order. */
  private outbox: string[] = [];
  /** Posts into the main frame's document. */
  private post: ((message: string) => void) | null = null;
  /** The main frame's address, as the web view last reported it. */
  private top: string | null = null;
  /** Whether the main frame's document has loaded, as its bridge script reported. */
  private loaded = false;
  /** Whether the main frame's current document reported itself (its script ran). */
  private reported = false;
  /** The hosts the user was asked about. */
  private readonly askedHosts = new Set<string>();

  constructor(
    private readonly store: Store,
    private readonly account: AccountId,
    private readonly roomId: string,
    private readonly target: WidgetTarget,
    readonly token: string,
    /** Whether to ask the user about each new host a meeting moves to (Android). */
    private readonly askHosts: boolean,
  ) {}

  snapshot = (): WidgetRunSnapshot => this.current;

  subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);

    return () => {
      this.listeners.delete(listener);
    };
  };

  private set(change: Partial<WidgetRunSnapshot>): void {
    this.current = { ...this.current, ...change };

    for (const listener of this.listeners) listener();
  }

  /** Starts the run (again, after a failure); one under way is cancelled first. */
  start(): void {
    this.cancel();
    this.access = undefined;

    const run = this.store.run(runCommand(this.account, this.roomId, this.target), (event) => {
      if (this.request !== run.request) return;

      switch (event.type) {
        case 'WidgetAccess':
          this.access = event.data.access;
          this.set({ view: { type: 'access', access: event.data.access } });
          break;
        case 'WidgetPage': {
          // Again for the same run: load the page again, its document and what waited gone.
          const page = event.data.page;
          const { view } = this.current;
          const load = view.type === 'page' ? view.load + 1 : 1;
          this.leavePage();
          this.askedHosts.clear();

          this.set({
            view: { type: 'page', page, load },
            captureHost: page.bridge === 'None' ? hostOf(page.url) : null,
            askingCapture: null,
          });
          break;
        }

        case 'WidgetMessage':
          this.deliver(event.data.message);
          break;
        case 'WidgetPermissions': {
          const { question, widget, permissions } = event.data;

          // Nothing asked: the engine withdrew that question (the page started over).
          if (permissions.length === 0) {
            if (this.current.asking?.question === question) this.set({ asking: null });
          } else {
            const ask: PermissionAsk = { request: run.request, question, widget, permissions };

            if (this.access) ask.context = this.access;
            this.set({ asking: ask });
          }

          break;
        }

        case 'OpenUrl':
          this.set({ view: { type: 'browser', url: event.data.url } });
          break;
        case 'IntegrationTerms':
          this.set({ view: { type: 'terms', policies: event.data.policies } });
          break;
        default:
          break;
      }
    });

    this.request = run.request;
    this.set({ view: { type: 'starting' }, asking: null, captureHost: null, askingCapture: null });
    run.then(
      () => this.finished(run.request),
      (cause: unknown) => this.failed(run.request, cause),
    );
  }

  /** Forgets the page's document and what waited for it. */
  private leavePage(): void {
    this.top = null;
    this.loaded = false;
    this.reported = false;
    this.outbox = [];
  }

  /** Whether the main frame holds a loaded document with exactly the page's origin. */
  private deliverable(): boolean {
    const { view } = this.current;

    return (
      this.post !== null &&
      this.loaded &&
      this.top !== null &&
      view.type === 'page' &&
      originOf(this.top) !== null &&
      originOf(this.top) === originOf(view.page.url)
    );
  }

  /** Posts an engine message into the page, or keeps it until the page's document is there. */
  private deliver(message: string): void {
    if (this.post !== null && this.deliverable()) this.post(message);
    else this.outbox.push(message);
  }

  /** Posts what waited, in order, once the page's document is there. */
  private flush(): void {
    const post = this.post;

    if (post === null || !this.deliverable()) return;
    const waiting = this.outbox;
    this.outbox = [];

    for (const message of waiting) post(message);
  }

  /** The web view that posts into the page's main frame, while it shows; `null` once gone. */
  attach(post: ((message: string) => void) | null): void {
    this.post = post;
    this.flush();
  }

  /**
   * The main frame's address, as the web view reports it (a load starting or ending, a
   * redirect, the History API): it decides where the engine's messages may go, and a meeting's
   * new host is asked about.
   */
  pageAt(url: string): void {
    const { view } = this.current;

    // A page with a bridge leaving its origin (Android may let a navigation through when its
    // check times out) is a new document: the engine starts over (`WidgetLoading`), and an open
    // question goes.
    if (view.type === 'page' && view.page.bridge !== 'None') {
      const origin = originOf(view.page.url);
      const was = this.top === null ? origin : originOf(this.top);

      if (was === origin && originOf(url) !== origin) {
        this.loaded = false;
        this.answerQuietly({ type: 'WidgetLoading' });
        this.set({ asking: null });
      }
    }

    this.top = url;
    this.followTop();
    this.flush();
  }

  /** The main frame starts loading a document at `url`: it hasn't reported itself yet. */
  pageStarted(url: string): void {
    this.reported = false;
    this.pageAt(url);
  }

  /**
   * The main frame finished loading `url`. Whether the bridge script must be installed now: a
   * page with a bridge, on its origin, whose document reported nothing (Android may run the
   * script late or not at all); installed again, it reports loading and loaded at once.
   */
  pageEnded(url: string): boolean {
    this.pageAt(url);
    const { view } = this.current;

    return (
      view.type === 'page' &&
      view.page.bridge !== 'None' &&
      !this.reported &&
      originOf(url) !== null &&
      originOf(url) === originOf(view.page.url)
    );
  }

  /** The page's document as its bridge script reported it: the engine hears it, then the page. */
  private lifecycle(stage: Lifecycle): void {
    this.reported = true;

    if (stage === 'loading') {
      this.loaded = false;
      this.answerQuietly({ type: 'WidgetLoading' });

      return;
    }

    this.loaded = true;
    this.answerQuietly({ type: 'WidgetLoaded' });
    this.flush();
  }

  /**
   * The page failed to load (`reason` names how, never where: an address may carry a token).
   * The run stops and the screen says so.
   */
  pageFailed(reason: string): void {
    const request = this.request;

    if (request === null) return;
    this.request = null;
    this.leavePage();
    this.store.send({ type: 'Cancel', data: request }, { quiet: true });
    this.set({ view: { type: 'failed', error: reason }, asking: null, askingCapture: null });
  }

  /** Asks about a meeting's new host, once, where the user is asked (`askHosts`). */
  private followTop(): void {
    const { view, captureHost } = this.current;

    if (!this.askHosts || view.type !== 'page' || view.page.bridge !== 'None' || this.top === null)
      return;
    const host = hostOf(this.top);

    if (host === null || host === captureHost || this.askedHosts.has(host)) return;
    this.askedHosts.add(host);
    this.set({ askingCapture: host });
  }

  /** The user's answer about `host`'s camera and microphone: allowed, it is the approved host. */
  answerCapture(host: string, allowed: boolean): void {
    if (this.current.askingCapture !== host) return;
    this.set(allowed ? { captureHost: host, askingCapture: null } : { askingCapture: null });
  }

  /**
   * A message the page posted, from the frame at `from`: taken only from the page's own origin
   * with the run's token. A lifecycle report is the document's; anything else is passed to the
   * engine when its bridge lets it through.
   */
  fromPage(raw: string, from: string): void {
    const { view } = this.current;

    if (this.request === null || view.type !== 'page' || !fromPage(view.page.url, from)) return;
    const stage = pageLifecycle(this.token, raw);

    if (stage !== null) {
      this.lifecycle(stage);

      return;
    }

    const message = pageMessage(view.page.bridge, this.token, raw);

    if (message !== null) this.answerQuietly({ type: 'WidgetMessage', data: message });
  }

  /** Answers the run; quiet, since an answer reaching a run that just ended has nobody to tell. */
  private answerQuietly(answer: Answer): void {
    if (this.request === null) return;
    this.store.send({ type: 'Answer', data: { request: this.request, answer } }, { quiet: true });
  }

  /**
   * Grants the permission question `ask` (the one the sheet was shown for) what the user allowed
   * (`[]` denies them all), if it is still the one waiting.
   */
  grant(ask: PermissionAsk, capabilities: string[]): void {
    const { asking } = this.current;

    // An answer from a sheet shown for another question (or run) is stale.
    if (
      this.request === null ||
      asking === null ||
      ask.request !== this.request ||
      asking.request !== ask.request ||
      asking.question !== ask.question
    )
      return;
    const asked = new Set(asking.permissions.map((p) => p.capability));
    const granted = capabilities.filter((capability) => asked.has(capability));

    answerRequest(this.store, this.request, {
      type: 'WidgetPermissions',
      data: { question: asking.question, granted },
    });
    this.set({ asking: null });
  }

  /** Confirms the action currently shown. */
  confirmAccess(): void {
    if (this.request === null || this.current.view.type !== 'access') return;
    answerRequest(this.store, this.request, { type: 'Continue' });
    this.set({ view: { type: 'starting' } });
  }

  /** Agrees to the integration manager's terms. */
  agree(): void {
    if (this.request === null || this.current.view.type !== 'terms') return;
    answerRequest(this.store, this.request, { type: 'Agree' });
    this.set({ view: { type: 'starting' } });
  }

  /** Stops the run under way, if any (the screen goes, or it opens in the browser). */
  cancel(): void {
    const request = this.request;
    this.leavePage();

    if (request === null) return;
    this.request = null;
    this.store.send({ type: 'Cancel', data: request }, { quiet: true });
  }

  private finished(request: RequestId): void {
    if (this.request !== request) return;
    this.request = null;
    this.leavePage();

    // Opened in the browser: the screen is going already.
    if (this.current.view.type !== 'browser') this.set({ view: { type: 'ended' }, asking: null });
  }

  private failed(request: RequestId, cause: unknown): void {
    if (this.request !== request) return;
    this.request = null;
    this.leavePage();

    const error =
      cause instanceof CommandFailed ? cause.message : cause instanceof Error ? cause.message : '';

    this.set({
      view:
        cause instanceof CommandFailed && cause.cancelled
          ? { type: 'ended' }
          : { type: 'failed', error },
      asking: null,
    });
  }
}

/**
 * The user's answer to a widget run's permission request outside the web view: allow them all,
 * deny them all, or `null` when the question was dismissed (the run is cancelled).
 */
export type PermissionAnswer = boolean | null;

/**
 * Opens one of a room's widgets in the browser with `open` (`RunWidget` with `browser`): a
 * meeting's address is the one that joins it. A meeting on a server that checks who joins may
 * first ask to confirm the user's identity to it (`WidgetPermissions`): `ask` puts it to the user,
 * and the run is answered, or cancelled when the question is dismissed, so it never hangs. A
 * question the engine withdraws (nothing asked) gets no answer.
 */
export async function openWidgetInBrowser(
  store: Store,
  account: AccountId,
  roomId: string,
  widgetId: string,
  open: (url: string) => void,
  ask: (widget: string, permissions: WidgetPermission[]) => Promise<PermissionAnswer>,
  confirm?: (access: WidgetAccess) => Promise<boolean>,
): Promise<void> {
  const command = { account, room_id: roomId, widget_id: widgetId, browser: true };

  // The questions the engine withdrew, by number, and the one showing: only its answer counts.
  const withdrawn = new Set<number>();
  let showing: number | null = null;

  const run = store.run({ type: 'RunWidget', data: command }, (event) => {
    if (event.type === 'OpenUrl') open(event.data.url);
    else if (event.type === 'WidgetAccess') {
      void (confirm?.(event.data.access) ?? Promise.resolve(false)).then((allowed) => {
        if (allowed) answerRequest(store, run.request, { type: 'Continue' });
        else store.send({ type: 'Cancel', data: run.request }, { quiet: true });
      });
    } else if (event.type === 'WidgetPermissions') {
      const { question } = event.data;
      const capabilities = event.data.permissions.map((permission) => permission.capability);

      // Nothing asked: the engine withdrew that question (an alert can't be taken back, but its
      // answer is dropped).
      if (capabilities.length === 0) {
        withdrawn.add(question);

        return;
      }

      showing = question;

      void ask(event.data.widget, event.data.permissions).then((allowed) => {
        if (withdrawn.has(question) || showing !== question) return;
        showing = null;

        if (allowed === null) {
          store.send({ type: 'Cancel', data: run.request }, { quiet: true });

          return;
        }

        const answer = {
          type: 'WidgetPermissions' as const,
          data: { question, granted: allowed ? capabilities : [] },
        };

        store.send({ type: 'Answer', data: { request: run.request, answer } }, { quiet: true });
      });
    }
  });

  await run;
}
