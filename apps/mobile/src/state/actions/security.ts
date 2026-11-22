// Encryption and sessions: verifying this session or another person (emoji or QR code), the
// recovery key, the offline device, exporting and importing message keys, resetting the
// identity, the account's other sessions, and the password and deactivation. Actions the server
// guards with the password share one retry (`runGuarded`), as the desktop's `Guarded` does.

import type {
  AccountId,
  Command,
  KeyImport,
  PendingContact,
  RequestId,
  VerificationStep,
} from '@better-matrix/matrix-native/types';

import { addNotice } from '../reducer';
import { CommandFailed } from '../requests';
import { activeAccount } from '../selectors';
import type { Store } from '../store';
import type { State, Verification } from '../types';

// ----- guarded actions ----------------------------------------------------------------------

/** An action the server may guard with the account password. New ones are new variants. */
export type Guarded =
  | { type: 'EndSession'; deviceId: string }
  | { type: 'ResetIdentity' }
  | { type: 'AddContact'; pending: PendingContact }
  | { type: 'ChangePassword'; newPassword: string }
  | { type: 'DeactivateAccount'; erase: boolean };

/** The command for a guarded action, with the password once the server asked for it. */
export function guardedCommand(
  account: AccountId,
  action: Guarded,
  password: string | null,
): Command {
  switch (action.type) {
    case 'EndSession':
      return { type: 'EndSession', data: { account, device_id: action.deviceId, password } };
    case 'ResetIdentity':
      return { type: 'ResetIdentity', data: { account, password } };
    case 'AddContact':
      return { type: 'AddContact', data: { account, pending: action.pending, password } };
    case 'ChangePassword':
      return {
        type: 'ChangePassword',
        data: { account, new_password: action.newPassword, password },
      };
    case 'DeactivateAccount':
      return { type: 'DeactivateAccount', data: { account, erase: action.erase, password } };
  }
}

export interface GuardHooks {
  /**
   * Asks the user for the account password; `error` is why the last one was refused. Resolves
   * `null` when the user gives up.
   */
  askPassword: (error: string | null) => Promise<string | null>;
  /** The server's page that approves the action (OAuth accounts): open it. */
  openUrl?: (url: string) => void;
  /** Each attempt's request, so the caller can stop it. */
  onRequest?: (request: RequestId) => void;
}

/**
 * Runs an action the server may guard with the password: when it asks, `askPassword` does and
 * the action goes again with it, until it succeeds or the user gives up. Resolves `true` when
 * done and `false` when the user gave up; rejects with `CommandFailed` when the server refused
 * for another reason (or the request was cancelled).
 */
export async function runGuarded(
  store: Store,
  account: AccountId,
  action: Guarded,
  hooks: GuardHooks,
): Promise<boolean> {
  let password: string | null = null;

  for (;;) {
    let needed = false;

    const run = store.run(guardedCommand(account, action, password), (event) => {
      if (event.type === 'PasswordNeeded') needed = true;
      else if (event.type === 'OpenUrl') hooks.openUrl?.(event.data.url);
    });

    hooks.onRequest?.(run.request);
    let refused: string | null;

    try {
      await run;

      return true;
    } catch (error) {
      // Asked for the password, the request ends with an error that isn't shown. Refused with
      // one, the error says why (a wrong password) and the user may try again.
      if (!(error instanceof CommandFailed) || error.cancelled || (!needed && password === null))
        throw error;
      refused = password === null ? null : error.message;
    }

    const next = await hooks.askPassword(refused);

    if (next === null) return false;
    password = next;
  }
}

// ----- verification -------------------------------------------------------------------------

/** The account on screen, when its session is known to be unverified: the banner offers to verify it. */
export function unverifiedAccount(state: State): AccountId | null {
  const account = activeAccount(state);

  return account?.security && !account.security.session_verified ? account.summary.id : null;
}

/**
 * One verification this device asks for (this session with the user's others, or with another
 * person), bound to the flow its command started: `VerificationRequested` names it before any of
 * its steps, so steps of other flows (a request coming in meanwhile) never drive or end it.
 * Views read `flowId()` and redraw on `subscribe`.
 */
export class AskedVerification {
  private request: RequestId | null = null;
  private flow: string | null = null;
  private closed = false;
  private readonly listeners = new Set<() => void>();

  constructor(
    private readonly store: Store,
    private readonly account: AccountId,
    /** The person to verify; `null` verifies this session. */
    private readonly userId: string | null,
  ) {}

  /** The flow the request started, once the engine named it. */
  flowId = (): string | null => this.flow;

  subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);

    return () => {
      this.listeners.delete(listener);
    };
  };

  /** Asks, once. */
  start(): void {
    if (this.request !== null || this.closed) return;

    const command: Command =
      this.userId === null
        ? { type: 'VerifyThisSession', data: this.account }
        : { type: 'VerifyUser', data: { account: this.account, user_id: this.userId } };

    const run = this.store.run(command, (event) => {
      if (event.type === 'VerificationRequested') this.started(event.data.flow_id);
    });

    this.request = run.request;
    run.catch((error: CommandFailed) => {
      if (!error.cancelled) this.store.update((s) => addNotice(s, error.message));
    });
  }

  private started(flowId: string): void {
    this.flow = flowId;

    // Closed while the request was on its way: the verification it started stops too.
    if (this.closed) {
      cancelVerification(this.store, this.account, flowId);

      return;
    }

    for (const listener of this.listeners) listener();
  }

  /**
   * Stops what is under way (leaving the sheet): the verification it started unless it is over,
   * or the request while it has none yet.
   */
  close(): void {
    if (this.closed) return;
    this.closed = true;
    const flowId = this.flow;

    if (flowId !== null) {
      const flow = this.store
        .getState()
        .verifications.find((v) => v.account === this.account && v.flowId === flowId);

      if (!flow || !verificationOver(flow.step))
        cancelVerification(this.store, this.account, flowId);
    }
    // Still on its way: the request isn't cancelled, as the server may have sent it already;
    // the flow it names is cancelled once it arrives.
  }
}

/** Asks what the user knows of someone's identity; `Identity` follows. */
export function loadIdentity(store: Store, account: AccountId, userId: string): void {
  store.send({ type: 'LoadIdentity', data: { account, user_id: userId } }, { quiet: true });
}

/** Switches a verification showing QR codes to comparing emoji. */
export function compareEmoji(store: Store, account: AccountId, flowId: string): void {
  store.send({ type: 'CompareEmoji', data: { account, flow_id: flowId } });
}

/** Hands a verification the other side's code, as the scanner read it. */
export function scanVerificationCode(
  store: Store,
  account: AccountId,
  flowId: string,
  code: number[],
): void {
  store.send({ type: 'ScanVerificationCode', data: { account, flow_id: flowId, code } });
}

export function acceptVerification(store: Store, account: AccountId, flowId: string): void {
  store.send({ type: 'AcceptVerification', data: { account, flow_id: flowId } });
}

/** Says whether the emoji match the other side's, or whether the other side scanned our code. */
export function confirmVerification(
  store: Store,
  account: AccountId,
  flowId: string,
  matches: boolean,
): void {
  store.send({ type: 'ConfirmVerification', data: { account, flow_id: flowId, matches } });
}

/** Stops a verification (declining one that came in). */
export function cancelVerification(store: Store, account: AccountId, flowId: string): void {
  store.send({ type: 'CancelVerification', data: { account, flow_id: flowId } }, { quiet: true });
}

/** Whether a verification is over: nothing left to stop. */
export function verificationOver(step: VerificationStep): boolean {
  return step.type === 'Done' || step.type === 'Cancelled';
}

/**
 * The verification the sheet follows: exactly `flowId` (the one it was opened for, or the one
 * its request started), nothing until that is known.
 */
export function followedVerification(
  verifications: Verification[],
  account: AccountId,
  flowId: string | null,
): Verification | undefined {
  return flowId === null
    ? undefined
    : verifications.find((v) => v.account === account && v.flowId === flowId);
}

/** Verification requests (from the user's other sessions or other people) that `seen` doesn't hold yet. */
export function newIncoming(
  verifications: Verification[],
  seen: ReadonlySet<string>,
): Verification[] {
  return verifications.filter((v) => v.step.type === 'Incoming' && !seen.has(verificationKey(v)));
}

/** A verification's key across accounts. */
export function verificationKey(v: { account: AccountId; flowId: string }): string {
  return `${v.account}|${v.flowId}`;
}

/** What the sheet can do at a step. */
export type VerificationAction =
  | 'accept'
  | 'decline'
  | 'cancel'
  | 'scan'
  | 'emoji'
  | 'scanned'
  | 'notScanned'
  | 'match'
  | 'mismatch'
  | 'done'
  | 'close';

export interface VerificationView {
  title:
    | 'verify.title'
    | 'verify.incoming_title'
    | 'verify.done_title'
    | 'mobile.accounts.verify_person_title'
    | 'mobile.accounts.verify_person_done_title';
  actions: VerificationAction[];
}

/**
 * How the verification sheet shows a step: `null` is the request on its way to a first step.
 * `person` when the other side is another person, not one of the user's sessions.
 */
export function verificationView(step: VerificationStep | null, person = false): VerificationView {
  const title = person ? 'mobile.accounts.verify_person_title' : 'verify.title';

  switch (step?.type) {
    case 'Incoming':
      return { title: 'verify.incoming_title', actions: ['decline', 'accept'] };
    case 'Ready':
      return {
        title,
        actions: [...(step.data.scan ? (['scan'] as const) : []), 'emoji', 'cancel'],
      };
    case 'Scanned':
      return { title, actions: ['notScanned', 'scanned'] };
    case 'Compare':
      return { title, actions: ['mismatch', 'match'] };
    case 'Done':
      return {
        title: person ? 'mobile.accounts.verify_person_done_title' : 'verify.done_title',
        actions: ['done'],
      };
    case 'Cancelled':
      return { title, actions: ['close'] };
    case 'Waiting':
    case 'Confirmed':
    case undefined:
      return { title, actions: ['cancel'] };
  }
}

// ----- recovery -----------------------------------------------------------------------------

/** Verifies this session by restoring secrets with the recovery key; rejects when it's wrong. */
export function recover(store: Store, account: AccountId, key: string): Promise<void> {
  return store.run({ type: 'Recover', data: { account, key: key.trim() } });
}

/**
 * Sets up recovery, or with `reset` replaces the key. The new key arrives once, in
 * `State.recoveryKey`, before this resolves.
 */
export function setUpRecovery(store: Store, account: AccountId, reset: boolean): Promise<void> {
  forgetRecoveryKey(store);

  return store.run(
    reset ? { type: 'ResetRecoveryKey', data: account } : { type: 'SetUpRecovery', data: account },
  );
}

/** Forgets the recovery key shown once. */
export function forgetRecoveryKey(store: Store): void {
  store.update((s): State => (s.recoveryKey === null ? s : { ...s, recoveryKey: null }));
}

/** The new recovery key for `account`, while it is shown. */
export function recoveryKeyFor(state: State, account: AccountId): string | null {
  return state.recoveryKey?.account === account ? state.recoveryKey.key : null;
}

// ----- identity reset -----------------------------------------------------------------------

/** Where an identity reset stands. The password, when asked for, is a prompt over it. */
export type ResetStep =
  /** What a reset keeps and costs; nothing has happened yet. */
  | { type: 'warning'; error: string | null }
  /** Sent. An OAuth account approves it on its account page, opened at `url`. */
  | { type: 'resetting'; request: RequestId | null; url: string | null }
  /** This session holds the new identity; recovery is next. */
  | { type: 'done' };

/**
 * Replaces the account's identity, after the warning. `onStep` hears each step; resolves with
 * the last one (`done`, or back at the warning with why, or without an error when the user gave
 * up).
 */
export async function resetIdentity(
  store: Store,
  account: AccountId,
  hooks: Pick<GuardHooks, 'askPassword'> & {
    openUrl: (url: string) => void;
    onStep: (step: ResetStep) => void;
  },
): Promise<ResetStep> {
  let step: ResetStep = { type: 'resetting', request: null, url: null };

  const move = (next: ResetStep) => {
    step = next;
    hooks.onStep(next);
  };

  move(step);

  try {
    const done = await runGuarded(
      store,
      account,
      { type: 'ResetIdentity' },
      {
        askPassword: hooks.askPassword,
        onRequest: (request) => move({ type: 'resetting', request, url: null }),
        openUrl: (url) => {
          if (step.type === 'resetting') move({ ...step, url });
          hooks.openUrl(url);
        },
      },
    );

    move(done ? { type: 'done' } : { type: 'warning', error: null });
  } catch (error) {
    const cancelled = error instanceof CommandFailed && error.cancelled;
    move({
      type: 'warning',
      error: cancelled ? null : error instanceof Error ? error.message : String(error),
    });
  }

  return step;
}

// ----- offline device, key export and import -----------------------------------------------

/**
 * Turns the offline device on (with the recovery key) or off; `SecurityChanged` follows. Rejects
 * with why when the server or the key refuses.
 */
export function setOfflineDevice(
  store: Store,
  account: AccountId,
  on: boolean,
  recoveryKey: string | null = null,
): Promise<void> {
  return store.run({
    type: 'SetOfflineDevice',
    data: { account, on, recovery_key: on ? (recoveryKey?.trim() ?? null) : null },
  });
}

/**
 * Asks for a secret (a key, a passphrase) and tries `attempt` with it until it is accepted;
 * `ask` hears why the last one was refused. Resolves with what `attempt` resolved with, or `null`
 * when the user gave up. A cancelled attempt ends it.
 */
export async function untilAccepted<T>(
  ask: (error: string | null) => Promise<string | null>,
  attempt: (secret: string) => Promise<T>,
): Promise<T | null> {
  let error: string | null = null;

  for (;;) {
    const secret = await ask(error);

    if (secret === null) return null;

    try {
      return await attempt(secret);
    } catch (cause) {
      if (!(cause instanceof CommandFailed) || cause.cancelled) throw cause;
      error = cause.message;
    }
  }
}

/** Turns the offline device on with the recovery key `ask` gets, asking again while it's wrong. */
export async function turnOnOfflineDevice(
  store: Store,
  account: AccountId,
  ask: (error: string | null) => Promise<string | null>,
): Promise<boolean> {
  const done = await untilAccepted(ask, (key) => setOfflineDevice(store, account, true, key));

  return done !== null;
}

/** Why a new passphrase or password and its confirmation can't be used, or `null` when they can. */
export function confirmationProblem(
  value: string,
  confirmation: string,
): 'empty' | 'mismatch' | null {
  if (value === '') return 'empty';

  return value === confirmation ? null : 'mismatch';
}

/** Writes this session's message keys to `path` (a plain path), encrypted with `passphrase`. */
export function exportRoomKeys(
  store: Store,
  account: AccountId,
  path: string,
  passphrase: string,
): Promise<void> {
  return store.run({ type: 'ExportRoomKeys', data: { account, path, passphrase } });
}

/**
 * Reads message keys from an exported file with the passphrase `ask` gets, asking again while
 * it's wrong; resolves with how many were new, or `null` when the user gave up.
 */
export function importRoomKeysAsking(
  store: Store,
  account: AccountId,
  path: string,
  ask: (error: string | null) => Promise<string | null>,
): Promise<KeyImport | null> {
  return untilAccepted(ask, (passphrase) => importRoomKeys(store, account, path, passphrase));
}

/** Reads message keys from an exported file; resolves with how many were new. */
export async function importRoomKeys(
  store: Store,
  account: AccountId,
  path: string,
  passphrase: string,
): Promise<KeyImport> {
  let found: KeyImport = { imported: 0, total: 0 };

  await store.run({ type: 'ImportRoomKeys', data: { account, path, passphrase } }, (event) => {
    if (event.type === 'KeysImported') found = event.data.import;
  });

  return found;
}

// ----- QR codes -----------------------------------------------------------------------------

/**
 * Reads the QR code in a camera picture (a plain path) in the engine, which keeps its bytes
 * whole; resolves with them, or `null` when no code reads.
 */
export async function readQrCode(store: Store, picture: string): Promise<number[] | null> {
  let code: number[] | null = null;

  await store.run({ type: 'ReadQrCode', data: { picture } }, (event) => {
    if (event.type === 'QrCodeRead') code = event.data.code;
  });

  return code;
}

// ----- password and deactivation ------------------------------------------------------------

/**
 * Changes a password account's password, sending the current one at once (the form asks for
 * it); rejects with why the server refused, such as a wrong current password.
 */
export function changePassword(
  store: Store,
  account: AccountId,
  current: string,
  newPassword: string,
): Promise<void> {
  return store.run(guardedCommand(account, { type: 'ChangePassword', newPassword }, current));
}

/**
 * Deactivates the account. A password account may be asked for the password; an OAuth account
 * deactivates on its account page (`openUrl`). Resolves `true` once the server deactivated it
 * here (the account is then removed) or the page opened, `false` when the user gave up.
 */
export function deactivateAccount(
  store: Store,
  account: AccountId,
  erase: boolean,
  hooks: GuardHooks,
): Promise<boolean> {
  return runGuarded(store, account, { type: 'DeactivateAccount', erase }, hooks);
}

// ----- sessions -----------------------------------------------------------------------------

/** Lists the account's signed-in sessions; `Sessions` follows. */
export function listSessions(store: Store, account: AccountId): void {
  store.send({ type: 'ListSessions', data: account });
}

export function renameSession(
  store: Store,
  account: AccountId,
  deviceId: string,
  name: string,
): void {
  const trimmed = name.trim();

  if (trimmed)
    store.send({ type: 'RenameSession', data: { account, device_id: deviceId, name: trimmed } });
}

/**
 * Signs another session out: a password account may be asked for the password, an OAuth account
 * ends it on the server's page (`openUrl`). The list is asked for again afterwards. A refusal
 * becomes a notice.
 */
export async function endSession(
  store: Store,
  account: AccountId,
  deviceId: string,
  hooks: GuardHooks,
): Promise<void> {
  try {
    await runGuarded(store, account, { type: 'EndSession', deviceId }, hooks);
  } catch (error) {
    if (error instanceof CommandFailed && !error.cancelled)
      store.update((s) => addNotice(s, error.message, account));
  }

  listSessions(store, account);
}
