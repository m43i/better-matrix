// Encryption and sessions: verifying this session (emoji or recovery key), the recovery key,
// resetting the identity, and the account's other sessions. Actions the server guards with the
// password share one retry (`runGuarded`), as the desktop's `Guarded` does.

import type {
  AccountId,
  Command,
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
  | { type: 'AddContact'; pending: PendingContact };

/** The command for a guarded action, with the password once the server asked for it. */
export function guardedCommand(account: AccountId, action: Guarded, password: string | null): Command {
  switch (action.type) {
    case 'EndSession':
      return { type: 'EndSession', data: { account, device_id: action.deviceId, password } };
    case 'ResetIdentity':
      return { type: 'ResetIdentity', data: { account, password } };
    case 'AddContact':
      return { type: 'AddContact', data: { account, pending: action.pending, password } };
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
export async function runGuarded(store: Store, account: AccountId, action: Guarded, hooks: GuardHooks): Promise<boolean> {
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
      if (!(error instanceof CommandFailed) || error.cancelled || (!needed && password === null)) throw error;
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

/** Asks the user's other sessions to verify this one; `Verification` events follow. */
export function verifyThisSession(store: Store, account: AccountId): RequestId {
  return store.send({ type: 'VerifyThisSession', data: account });
}

export function acceptVerification(store: Store, account: AccountId, flowId: string): void {
  store.send({ type: 'AcceptVerification', data: { account, flow_id: flowId } });
}

/** Says whether the emoji match the other session's. */
export function confirmVerification(store: Store, account: AccountId, flowId: string, matches: boolean): void {
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
 * The verification the sheet follows: `flowId` when it was opened for one, else the first of
 * the account's verifications that wasn't there when it opened (`known`), the one it started.
 */
export function followedVerification(
  verifications: Verification[],
  account: AccountId,
  flowId: string | null,
  known: readonly string[],
): Verification | undefined {
  return verifications.find(
    (v) => v.account === account && (flowId !== null ? v.flowId === flowId : !known.includes(v.flowId)),
  );
}

/** Verification requests from the user's other sessions that `seen` doesn't hold yet. */
export function newIncoming(verifications: Verification[], seen: ReadonlySet<string>): Verification[] {
  return verifications.filter((v) => v.step.type === 'Incoming' && !seen.has(verificationKey(v)));
}

/** A verification's key across accounts. */
export function verificationKey(v: { account: AccountId; flowId: string }): string {
  return `${v.account}|${v.flowId}`;
}

/** What the sheet can do at a step. */
export type VerificationAction = 'accept' | 'decline' | 'cancel' | 'match' | 'mismatch' | 'done' | 'close';

/** How the verification sheet shows a step: `null` is the request on its way to a first step. */
export function verificationView(step: VerificationStep | null): {
  title: 'verify.title' | 'verify.incoming_title' | 'verify.done_title';
  actions: VerificationAction[];
} {
  switch (step?.type) {
    case 'Incoming':
      return { title: 'verify.incoming_title', actions: ['decline', 'accept'] };
    case 'Compare':
      return { title: 'verify.title', actions: ['mismatch', 'match'] };
    case 'Done':
      return { title: 'verify.done_title', actions: ['done'] };
    case 'Cancelled':
      return { title: 'verify.title', actions: ['close'] };
    case 'Waiting':
    case 'Confirmed':
    case undefined:
      return { title: 'verify.title', actions: ['cancel'] };
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
  return store.run(reset ? { type: 'ResetRecoveryKey', data: account } : { type: 'SetUpRecovery', data: account });
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
  hooks: Pick<GuardHooks, 'askPassword'> & { openUrl: (url: string) => void; onStep: (step: ResetStep) => void },
): Promise<ResetStep> {
  let step: ResetStep = { type: 'resetting', request: null, url: null };
  const move = (next: ResetStep) => {
    step = next;
    hooks.onStep(next);
  };
  move(step);
  try {
    const done = await runGuarded(store, account, { type: 'ResetIdentity' }, {
      askPassword: hooks.askPassword,
      onRequest: (request) => move({ type: 'resetting', request, url: null }),
      openUrl: (url) => {
        if (step.type === 'resetting') move({ ...step, url });
        hooks.openUrl(url);
      },
    });
    move(done ? { type: 'done' } : { type: 'warning', error: null });
  } catch (error) {
    const cancelled = error instanceof CommandFailed && error.cancelled;
    move({ type: 'warning', error: cancelled ? null : error instanceof Error ? error.message : String(error) });
  }
  return step;
}

// ----- sessions -----------------------------------------------------------------------------

/** Lists the account's signed-in sessions; `Sessions` follows. */
export function listSessions(store: Store, account: AccountId): void {
  store.send({ type: 'ListSessions', data: account });
}

export function renameSession(store: Store, account: AccountId, deviceId: string, name: string): void {
  const trimmed = name.trim();
  if (trimmed) store.send({ type: 'RenameSession', data: { account, device_id: deviceId, name: trimmed } });
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
    if (error instanceof CommandFailed && !error.cancelled) store.update((s) => addNotice(s, error.message, account));
  }
  listSessions(store, account);
}
