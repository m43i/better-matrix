// Accounts: signing in (a two-step state machine, as the desktop's `SignInStep`), signing out,
// sessions the server ended, the profile and its contact details, and per-account settings.

import type { EngineEvent } from '@better-matrix/matrix-native/client';
import type {
  AccountId,
  AccountPage,
  Answer,
  Command,
  ContactDetail,
  ContactKind,
  Homeserver,
  NotificationMode,
  PendingContact,
  RegistrationStep,
  RequestId,
  Security,
  Status,
} from '@better-matrix/matrix-native/types';

import { OAUTH_REDIRECT } from '../../engine/identity';
import { PUSH_APP_ID } from '../../engine/pushRules';
import { t } from '../../i18n';
import { addNotice, updateAccount } from '../reducer';
import { CommandFailed } from '../requests';
import type { Store } from '../store';
import type { State } from '../types';
import { setActive } from './core';
import { type GuardHooks, runGuarded } from './security';

// ----- signing in ---------------------------------------------------------------------------

/** What the homeserver field names: a server to discover, and a username from a full Matrix ID. */
export interface SignInInput {
  server: string;
  username: string | null;
}

/** Parses the homeserver field, as `matrix_core::SignInInput::parse`; `null` when it names no server. */
export function parseSignInInput(input: string): SignInInput | null {
  const trimmed = input.trim();
  let server = trimmed;
  let username: string | null = null;

  if (trimmed.startsWith('@')) {
    const colon = trimmed.indexOf(':');

    if (colon >= 0) {
      server = trimmed.slice(colon + 1);
      username = trimmed.slice(1, colon) || null;
    }
  }

  server = server.replace(/\/+$/, '');

  return server ? { server, username } : null;
}

/** A homeserver's short name: its server name, else its URL's host. */
export function serverName(server: Homeserver): string {
  return server.server_name ?? serverHost(server);
}

/** The host a homeserver's requests go to. */
export function serverHost(server: Homeserver): string {
  return server.url.replace(/^https?:\/\//, '').replace(/\/+$/, '');
}

/** Where the sign-in is. Each step holds only what is valid in it. */
export type SignInStep =
  /** Step 1: choosing a homeserver. */
  | { type: 'server'; error: string | null }
  /** Step 1: resolving it. */
  | { type: 'discovering'; request: RequestId; server: string }
  /** Step 2 on an OAuth server: the user finishes in the browser. `request` is null after a failure. */
  | {
      type: 'browser';
      server: Homeserver;
      request: RequestId | null;
      url: string | null;
      error: string | null;
    }
  /** Step 2 on a password server. `submitting` is the login in flight. */
  | { type: 'password'; server: Homeserver; submitting: RequestId | null; error: string | null }
  /**
   * Creating an account on a password server: the form, then what the server asks
   * (`question`) while `request` runs.
   */
  | {
      type: 'register';
      server: Homeserver;
      request: RequestId | null;
      question: RegistrationStep | null;
      error: string | null;
    }
  /** Signed in: the account was added. */
  | { type: 'done'; account: AccountId | null };

/** The request a step waits on, if any. */
export function pendingRequest(step: SignInStep): RequestId | null {
  switch (step.type) {
    case 'discovering':
      return step.request;
    case 'browser':
      return step.request;
    case 'password':
      return step.submitting;
    case 'register':
      return step.request;
    case 'server':
    case 'done':
      return null;
  }
}

/** Records a failed request: discovery goes back to step 1, a sign-in stays on step 2. */
export function failStep(step: SignInStep, request: RequestId, message: string | null): SignInStep {
  switch (step.type) {
    case 'discovering':
      return step.request === request ? { type: 'server', error: message } : step;
    case 'browser':
      return step.request === request ? { ...step, request: null, error: message } : step;
    case 'password':
      return step.submitting === request ? { ...step, submitting: null, error: message } : step;
    case 'register':
      return step.request === request
        ? { ...step, request: null, question: null, error: message }
        : step;
    case 'server':
    case 'done':
      return step;
  }
}

/** Step 2 for a discovered server: the browser for OAuth, else the password form, else an error. */
export function stepFor(
  server: Homeserver,
): 'browser' | 'password' | { type: 'server'; error: string } {
  if (server.methods.oauth) return 'browser';

  if (server.methods.password) return 'password';

  return { type: 'server', error: t('sign_in.no_method', { server: serverName(server) }) };
}

/** Which of step 2's other ways in it offers. */
export interface OtherWaysOffered {
  create: boolean;
  qr: boolean;
}

/**
 * Step 2's other ways in besides signing in: creating an account and a QR code, where the server
 * takes them. Signing in again as a suspended account (`resume`) offers neither: both make a new
 * store, and a QR sign-in as that account would replace the data it kept.
 */
export function otherWays(server: Homeserver, resume: AccountId | null): OtherWaysOffered {
  return {
    create: resume === null && server.methods.create_account,
    qr: resume === null && server.methods.qr,
  };
}

/** An email address as typed, trimmed; `null` when it can't be one. */
export function parseEmail(text: string): string | null {
  const trimmed = text.trim();

  return /^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(trimmed) ? trimmed : null;
}

/** Opens the sign-in page in a browser session that returns to the app; the URL it came back with. */
export type AuthBrowser = (url: string, redirect: string) => Promise<string | null>;

export interface SignInSnapshot {
  step: SignInStep;
  /** The username a full Matrix ID filled in, for the password form. */
  username: string;
  /** The account signed out softly that this sign-in takes back, keeping its data. */
  resume: AccountId | null;
}

/**
 * One sign-in, from the homeserver to the added account: the mobile form of the desktop's
 * `SignInForm`. Views read `snapshot()` and redraw on `subscribe`.
 */
export class SignInFlow {
  private state: SignInSnapshot = {
    step: { type: 'server', error: null },
    username: '',
    resume: null,
  };
  private readonly listeners = new Set<() => void>();
  /** The account each sign-in command signed in, from its `SignedIn`. */
  private readonly signedIn = new Map<RequestId, AccountId>();

  /** Keeps the account a sign-in command names as the one it signed in. */
  private readonly hear = (event: EngineEvent): void => {
    if (event.type === 'SignedIn') this.signedIn.set(event.data.request, event.data.account);
  };

  constructor(
    private readonly store: Store,
    private readonly browser: AuthBrowser,
    /** Opens a page in the in-app browser: a registration step the server runs itself. */
    private readonly openPage: (url: string) => void = () => undefined,
  ) {}

  snapshot = (): SignInSnapshot => this.state;

  subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);

    return () => {
      this.listeners.delete(listener);
    };
  };

  private set(change: Partial<SignInSnapshot>): void {
    this.state = { ...this.state, ...change };

    for (const listener of this.listeners) listener();
  }

  private setStep(step: SignInStep): void {
    this.set({ step });
  }

  /**
   * Step 1: resolve what the user typed. A full Matrix ID also fills in the username. With
   * `create`, step 2 creates an account where the server allows it.
   */
  submitServer(text: string, create = false): void {
    if (pendingRequest(this.state.step) !== null) return;
    const input = parseSignInInput(text);

    if (!input) {
      this.setStep({ type: 'server', error: t('sign_in.enter_server') });

      return;
    }

    // Signing in as an account signed out softly takes back its data, however it started.
    const typed = text.trim().toLowerCase();

    const resume =
      this.store.getState().ended.find((e) => e.kept && e.account.toLowerCase() === typed)
        ?.account ?? null;

    const run = this.store.run({ type: 'Discover', data: { server: input.server } }, (event) => {
      if (event.type === 'HomeserverDiscovered')
        this.discovered(event.data.request, event.data.homeserver, create);
    });

    this.set({ username: input.username ?? this.state.username, resume });
    this.setStep({ type: 'discovering', request: run.request, server: input.server });
    run.catch((cause: unknown) => this.failed(run.request, cause));
  }

  private discovered(request: RequestId, server: Homeserver, create: boolean): void {
    const step = this.state.step;

    if (step.type !== 'discovering' || step.request !== request) return;

    if (create && otherWays(server, this.state.resume).create) {
      this.create(server);

      return;
    }

    const next = stepFor(server);

    if (next === 'browser') this.startBrowser(server);
    else if (next === 'password')
      this.setStep({ type: 'password', server, submitting: null, error: null });
    else this.setStep(next);
  }

  /** Signs in (or with `create`, creates an account first) on the server's page. */
  private startBrowser(server: Homeserver, create = false): void {
    const command: Command = {
      type: 'LoginOAuth',
      data: { homeserver: server.url, resume: this.state.resume, create },
    };

    const run = this.store.run(command, (event) => {
      this.hear(event);

      if (event.type !== 'OpenUrl') return;
      const step = this.state.step;

      if (step.type !== 'browser' || step.request !== run.request) return;
      this.setStep({ ...step, url: event.data.url });
      void this.openBrowser(run.request, event.data.url);
    });

    this.setStep({ type: 'browser', server, request: run.request, url: null, error: null });
    run.then(
      () => this.finished(run.request),
      (cause: unknown) => this.failed(run.request, cause),
    );
  }

  /** Opens the sign-in page; its answer finishes the login, closing it without one cancels it. */
  private async openBrowser(request: RequestId, url: string): Promise<void> {
    let back: string | null = null;

    try {
      back = await this.browser(url, OAUTH_REDIRECT);
    } catch (error) {
      this.store.update((s) => addNotice(s, t('notice.browser', { error: String(error) })));
    }

    if (pendingRequest(this.state.step) !== request) return;

    if (back !== null) {
      this.store.send({ type: 'FinishOAuth', data: { request, url: back } }, { quiet: true });
    } else {
      this.store.send({ type: 'Cancel', data: request }, { quiet: true });
    }
  }

  /** Opens the sign-in page again, for a browser the user closed by mistake. */
  reopenBrowser(): void {
    const step = this.state.step;

    if (step.type === 'browser' && step.request !== null && step.url !== null) {
      void this.openBrowser(step.request, step.url);
    }
  }

  /** Starts the browser sign-in again, after a failure or from the password form. */
  retryBrowser(): void {
    const step = this.state.step;

    if ((step.type !== 'browser' && step.type !== 'password') || !step.server.methods.oauth) return;
    this.cancelPending();
    this.startBrowser(step.server);
  }

  /**
   * Creates an account instead of signing in: on the server's page for OAuth servers, else with
   * the registration form.
   */
  createAccount(): void {
    const step = this.state.step;

    if (step.type !== 'browser' && step.type !== 'password') return;

    if (!otherWays(step.server, this.state.resume).create) return;
    this.cancelPending();
    this.create(step.server);
  }

  private create(server: Homeserver): void {
    if (server.methods.oauth) this.startBrowser(server, true);
    else this.setStep({ type: 'register', server, request: null, question: null, error: null });
  }

  /** Creates the account the registration form describes. */
  submitRegistration(username: string, password: string, confirmation: string): void {
    const step = this.state.step;

    if (step.type !== 'register' || step.request !== null) return;
    const user = username.trim();

    if (!user || !password) {
      this.setStep({ ...step, error: t('sign_in.fill_in') });

      return;
    }

    if (password !== confirmation) {
      this.setStep({ ...step, error: t('mobile.accounts.register_mismatch') });

      return;
    }

    const run = this.store.run(
      { type: 'Register', data: { homeserver: step.server.url, username: user, password } },
      (event) => {
        this.hear(event);
        const current = this.state.step;

        if (current.type !== 'register' || current.request !== run.request) return;

        if (event.type === 'Registration') this.setStep({ ...current, question: event.data.step });
        else if (event.type === 'OpenUrl') this.openPage(event.data.url);
      },
    );

    this.set({ username: user, step: { ...step, request: run.request, error: null } });
    run.then(
      () => this.finished(run.request),
      (cause: unknown) => this.failed(run.request, cause),
    );
  }

  /** Answers what the server asked while creating the account; an email address must be one. */
  answerRegistration(answer: Answer): void {
    const step = this.state.step;

    if (step.type !== 'register' || step.request === null || step.question === null) return;

    const address = answer.type === 'Email' ? parseEmail(answer.data) : null;

    if (answer.type === 'Email' && address === null) {
      this.setStep({ ...step, error: t('mobile.accounts.registration_email_invalid') });

      return;
    }

    answerRequest(
      this.store,
      step.request,
      address === null ? answer : { type: 'Email', data: address },
    );
    this.setStep({ ...step, question: null, error: null });
  }

  /** Opens the server's page for a registration step again. */
  reopenRegistrationPage(): void {
    const step = this.state.step;

    if (step.type === 'register' && step.question?.type === 'Browser')
      this.openPage(step.question.data.url);
  }

  /** Switches step 2 to the password form, on a server that offers both. */
  usePassword(): void {
    const step = this.state.step;

    if (
      (step.type !== 'browser' && step.type !== 'password' && step.type !== 'register') ||
      !step.server.methods.password
    )
      return;
    this.cancelPending();
    this.setStep({ type: 'password', server: step.server, submitting: null, error: null });
  }

  /** Back to step 1. Another server is another account: nothing suspended carries over. */
  changeServer(): void {
    this.cancelPending();
    this.set({ step: { type: 'server', error: null }, resume: null });
  }

  /** Signs in with a password. */
  submitPassword(username: string, password: string): void {
    const step = this.state.step;

    if (step.type !== 'password' || step.submitting !== null) return;
    const user = username.trim();

    if (!user || !password) {
      this.setStep({ ...step, error: t('sign_in.fill_in') });

      return;
    }

    const run = this.store.run(
      {
        type: 'Login',
        data: { homeserver: step.server.url, username: user, password, resume: this.state.resume },
      },
      this.hear,
    );

    this.set({ username: user, step: { ...step, submitting: run.request, error: null } });
    run.then(
      () => this.finished(run.request),
      (cause: unknown) => this.failed(run.request, cause),
    );
  }

  /** Stops whatever the sign-in waits on, leaving the step as it is (leaving the screen). */
  cancelPending(): void {
    const request = pendingRequest(this.state.step);

    if (request !== null) {
      this.store.send({ type: 'Cancel', data: request }, { quiet: true });
      this.setStep(failStep(this.state.step, request, null));
    }
  }

  /** The login finished: the new account goes on screen. */
  private finished(request: RequestId): void {
    if (pendingRequest(this.state.step) !== request) return;
    const accounts = this.store.getState().accounts.map((a) => a.summary.id);
    const added = this.signedIn.get(request) ?? null;

    this.signedIn.delete(request);

    if (added !== null && accounts.includes(added)) setActive(this.store, added);
    this.setStep({ type: 'done', account: added });
  }

  private failed(request: RequestId, cause: unknown): void {
    // A cancelled request says nothing: the user did it, or another step replaced it.
    const message =
      cause instanceof CommandFailed ? (cause.cancelled ? null : cause.message) : String(cause);

    this.setStep(failStep(this.state.step, request, message));
  }
}

// ----- signing out and ended sessions -------------------------------------------------------

/**
 * Signs an account out on the server and forgets it here, its data included. Its pusher on this
 * device goes first (the server drops it with the session anyway; this doesn't wait on that).
 */
export function signOut(store: Store, account: AccountId): void {
  const pusher = store.getState().pushers[account];

  if (!pusher) {
    store.send({ type: 'Logout', data: account });

    return;
  }

  store
    .run({ type: 'RemovePusher', data: { account, app_id: PUSH_APP_ID, push_key: pusher.token } })
    .catch(() => undefined)
    .finally(() => store.send({ type: 'Logout', data: account }));
}

/**
 * Forgets an account whose session the server ended. One signed out softly still keeps its
 * data here, which goes with it.
 */
export function removeEnded(store: Store, account: AccountId): void {
  const ended = store.getState().ended.find((e) => e.account === account);

  if (!ended) return;
  store.update((s): State => ({ ...s, ended: s.ended.filter((e) => e.account !== account) }));

  if (ended.kept) store.send({ type: 'Logout', data: account });
}

/**
 * What the app does when the engine reports these, as the desktop's `State::apply` does: an
 * ended session stops its account (signed out softly, its data stays for signing in again), and
 * a newly verified session fetches the keys it could not read before.
 */
export function installAccountEffects(store: Store): () => void {
  // Every refused request reports the ended session again: only the first counts.
  const stopped = new Set<AccountId>();
  const lastSecurity = new Map<AccountId, Security>();

  return store.addEffect((event: EngineEvent) => {
    switch (event.type) {
      case 'AccountAdded':
        stopped.delete(event.data.id);

        return;
      case 'AccountRemoved':
        lastSecurity.delete(event.data);

        return;
      case 'SessionEnded': {
        const { account, kept } = event.data;

        if (stopped.has(account)) return;
        stopped.add(account);

        if (!kept) store.send({ type: 'Logout', data: account });
        // One suspended in an earlier run is stopped already.
        else if (store.getState().accounts.some((a) => a.summary.id === account)) {
          store.send({ type: 'Suspend', data: account });
        }

        return;
      }

      case 'SecurityChanged': {
        // Newly verified, or the backup key just arrived: the backup can now fill in the keys of
        // messages that couldn't be decrypted.
        const { account, security } = event.data;
        const before = lastSecurity.get(account);
        lastSecurity.set(account, security);

        const changed =
          before?.session_verified !== security.session_verified ||
          before.recovery !== security.recovery;

        if (security.session_verified && changed)
          store.send({ type: 'FetchRoomKeys', data: account }, { quiet: true });

        return;
      }

      default:
        return;
    }
  });
}

/** Fetches the keys of the rooms on screen from the key backup, after verifying. */
export function fetchRoomKeys(store: Store, account: AccountId): void {
  store.send({ type: 'FetchRoomKeys', data: account });
}

// ----- settings and profile -----------------------------------------------------------------

/** When the account's rooms notify unless they set their own mode. */
export function setDefaultNotificationMode(
  store: Store,
  account: AccountId,
  mode: NotificationMode,
): void {
  store.send({ type: 'SetDefaultNotificationMode', data: { account, mode } });
}

/** Keywords sorted as the engine reports them: case-insensitively. */
function sortedKeywords(keywords: string[]): string[] {
  return [...keywords].sort((a, b) => {
    const [x, y] = [a.toLowerCase(), b.toLowerCase()];

    if (x !== y) return x < y ? -1 : 1;

    return a < b ? -1 : a > b ? 1 : 0;
  });
}

/** Sends a keyword change; a failure tells the user, and the engine reports the keywords again. */
function sendKeyword(store: Store, account: AccountId, keyword: string, enabled: boolean): void {
  store.run({ type: 'SetKeyword', data: { account, keyword, enabled } }).catch((cause: unknown) => {
    if (cause instanceof CommandFailed && cause.cancelled) return;
    const message = cause instanceof Error ? cause.message : String(cause);
    store.update((s) =>
      addNotice(s, t('notice.notification_setting', { error: message }), account),
    );
  });
}

/**
 * Adds a word that notifies the user wherever it is written, shown at once. Blank words and
 * ones the account has already are left alone; returns whether it was added.
 */
export function addKeyword(store: Store, account: AccountId, keyword: string): boolean {
  const word = keyword.trim();
  const known = store.getState().accounts.find((a) => a.summary.id === account)?.keywords ?? [];

  if (!word) return false;

  // Sent even when listed: a sync older than a removal may still list it, and the engine checks
  // the server's rules.
  if (!known.includes(word)) {
    store.update((s) =>
      updateAccount(s, account, (a) => ({
        ...a,
        keywords: sortedKeywords([...(a.keywords ?? []), word]),
      })),
    );
  }

  sendKeyword(store, account, word, true);

  return true;
}

/** Removes one of the account's notification keywords, shown gone at once. */
export function removeKeyword(store: Store, account: AccountId, keyword: string): void {
  store.update((s) =>
    updateAccount(s, account, (a) => ({
      ...a,
      keywords: (a.keywords ?? []).filter((known) => known !== keyword),
    })),
  );
  sendKeyword(store, account, keyword, false);
}

/**
 * Sets what others see of the user on an account (online, away, hidden), showing it at once;
 * if the server refuses, the saved status shows again (unless another was chosen since) with a
 * notice.
 */
export function setStatus(store: Store, account: AccountId, status: Status): void {
  const shown = () => store.getState().accounts.find((a) => a.summary.id === account)?.status;

  if (shown() === undefined || shown() === status) return;

  store.update((s) => updateAccount(s, account, (a) => ({ ...a, status })));
  store.run({ type: 'SetStatus', data: { account, status } }).catch((error: CommandFailed) => {
    // Another status chosen or reported since stays.
    if (shown() === status) {
      store.update((s) => updateAccount(s, account, (a) => ({ ...a, status: a.savedStatus })));
    }

    store.update((s) => addNotice(s, t('notice.status', { error: error.message })));
  });
}

/** Saves or clears the account's status message. The engine owns its optional expiry. */
export async function setStatusMessage(
  store: Store,
  account: AccountId,
  text: string,
  clearAt: number | null,
): Promise<void> {
  const message = Array.from(text.trim()).slice(0, 100).join('').trimEnd() || null;
  await store.run({
    type: 'SetStatusMessage',
    data: {
      account,
      message,
      clear_at_ms: message && clearAt !== null && clearAt > Date.now() ? clearAt : null,
    },
  });
}

/** The display name the profile's field starts with: empty when the account has none. */
export function editableName(account: AccountId, displayName: string): string {
  return displayName === account ? '' : displayName;
}

/** Whether `name` would change the saved display name. */
export function nameChanged(saved: string, name: string): boolean {
  const trimmed = name.trim();

  return trimmed !== '' && trimmed !== saved;
}

/** Changes the display name; resolves once saved. */
export function saveDisplayName(store: Store, account: AccountId, name: string): Promise<void> {
  return store.run({ type: 'SetDisplayName', data: { account, name: name.trim() } });
}

/** Uploads a picture (a plain file path) as the profile picture, or with `null` removes it. */
export function setAvatar(store: Store, account: AccountId, file: string | null): Promise<void> {
  return store.run({ type: 'SetAvatar', data: { account, file } });
}

/** Lists the email addresses and phone numbers linked to the account. */
export function listContactDetails(store: Store, account: AccountId): void {
  store.send({ type: 'ListContactDetails', data: account });
}

/** Asks the server to send a link (email) or code (phone); resolves with the detail to confirm. */
export async function requestContactCode(
  store: Store,
  account: AccountId,
  kind: ContactKind,
  address: string,
  country: string,
): Promise<PendingContact> {
  let pending: PendingContact | null = null;

  const run = store.run(
    { type: 'RequestContactCode', data: { account, kind, address: address.trim(), country } },
    (event) => {
      if (event.type === 'ContactCodeSent') pending = event.data.pending;
    },
  );

  await run;

  if (pending === null) throw new CommandFailed(t('mobile.accounts.no_code'), run.request);

  return pending;
}

/** Sends the code texted to a new phone number. */
export function confirmContactCode(
  store: Store,
  account: AccountId,
  pending: PendingContact,
  code: string,
): Promise<void> {
  return store.run({ type: 'ConfirmContactCode', data: { account, pending, code: code.trim() } });
}

/** Links a confirmed contact detail; the server usually asks for the password first. */
export function addContact(
  store: Store,
  account: AccountId,
  pending: PendingContact,
  hooks: GuardHooks,
): Promise<boolean> {
  return runGuarded(store, account, { type: 'AddContact', pending }, hooks);
}

/** Unlinks a contact detail. */
export function removeContact(store: Store, account: AccountId, contact: ContactDetail): void {
  store.send({ type: 'RemoveContact', data: { account, contact } });
}

/** How a contact detail reads: phone numbers get their `+` back. */
export function contactLabel(contact: ContactDetail): string {
  return contact.kind === 'Phone' ? `+${contact.address}` : contact.address;
}

/** Opens a page of the homeserver's account management (OAuth accounts) with `open`. */
export async function openAccountPage(
  store: Store,
  account: AccountId,
  open: (url: string) => void,
  page: AccountPage = 'Profile',
): Promise<void> {
  await store.run({ type: 'OpenAccountPage', data: { account, page } }, (event) => {
    if (event.type === 'OpenUrl') open(event.data.url);
  });
}

/** Answers a question a running command asked (a check code, a registration step). */
export function answerRequest(store: Store, request: RequestId, answer: Answer): void {
  store.send({ type: 'Answer', data: { request, answer } });
}
