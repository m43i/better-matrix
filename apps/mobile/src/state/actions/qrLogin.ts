// Signing a device in with a QR code (MSC4108), on either side: a new device signing in
// (`SignInWithQr`) or a signed-in, verified session approving one (`LinkDevice`). Both report
// the same `QrLoginStep`s, so one flow drives both screens; the command it starts is the only
// difference (`QrLoginStart`).

import type {
  AccountId,
  Command,
  Homeserver,
  QrImage,
  QrLoginStep,
  RequestId,
} from '@better-matrix/matrix-native/types';

import { CommandFailed } from '../requests';
import type { Store } from '../store';
import { answerRequest } from './accounts';
import { setActive } from './core';

/** What a QR sign-in screen shows. */
export type QrLoginView =
  /** The camera is open for the other device's code. */
  | { type: 'scanning' }
  /** Talking to the server. */
  | { type: 'starting' }
  /** This device's code, for the other one to scan. */
  | { type: 'show'; qr: QrImage }
  /** The other device shows two digits to type here; `sent` once they went. */
  | { type: 'enterCheckCode'; sent: boolean }
  /** Two digits to type on the other device. */
  | { type: 'checkCode'; code: number }
  /** Approve the new device on the account page (opened in the browser). */
  | { type: 'approve'; url: string }
  /** The new device waits for the approval; the account page shows `code`. */
  | { type: 'waiting'; code: string | null }
  /** The secrets are on their way to the new device. */
  | { type: 'syncing' }
  /**
   * The engine reported `Done`, but the command hasn't finished: leaving now would cancel a
   * sign-in that is being saved, so only `done` (the command finished) lets the screen go.
   */
  | { type: 'finishing'; account: AccountId | null }
  /** Signed in, the command finished; `account` is the account a new device added. */
  | { type: 'done'; account: AccountId | null }
  /** It stopped, with why. */
  | { type: 'failed'; error: string };

/** The view for a step the engine reported. */
export function qrLoginView(step: QrLoginStep): QrLoginView {
  switch (step.type) {
    case 'Starting':
      return { type: 'starting' };
    case 'Show':
      return { type: 'show', qr: step.data };
    case 'EnterCheckCode':
      return { type: 'enterCheckCode', sent: false };
    case 'CheckCode':
      return { type: 'checkCode', code: step.data };
    case 'Approve':
      return { type: 'approve', url: step.data.url };
    case 'Waiting':
      return { type: 'waiting', code: step.data.code };
    case 'Syncing':
      return { type: 'syncing' };
    case 'Done':
      return { type: 'finishing', account: step.data.account };
  }
}

/** The two digits typed from the other device, or `null` when they aren't two digits. */
export function parseCheckCode(text: string): number | null {
  const trimmed = text.trim();

  return /^\d{2}$/.test(trimmed) ? Number(trimmed) : null;
}

/** A check code as the other device shows it: always two digits. */
export function formatCheckCode(code: number): string {
  return String(code).padStart(2, '0');
}

/** The commands that start the exchange: with the other device's code, or showing this one's. */
export interface QrLoginStart {
  scanned: (code: number[]) => Command;
  /** `null` where this device can't show a code. */
  shown: Command | null;
}

/**
 * A new device signing in: scanning a signed-in device's code (which names the server), or
 * showing one through `homeserver` when it offers QR sign-in (`LoginMethods::qr`).
 */
export function signInWithQr(homeserver: Homeserver | null): QrLoginStart {
  return {
    scanned: (code) => ({ type: 'SignInWithQr', data: { type: 'Scanned', data: code } }),
    shown:
      homeserver?.methods.qr === true
        ? { type: 'SignInWithQr', data: { type: 'Show', data: { homeserver: homeserver.url } } }
        : null,
  };
}

/** A signed-in session approving a new device: showing a code, or with the one it scanned. */
export function linkDevice(account: AccountId): QrLoginStart {
  return {
    scanned: (code) => ({ type: 'LinkDevice', data: { account, scanned: code } }),
    shown: { type: 'LinkDevice', data: { account, scanned: null } },
  };
}

/**
 * One QR sign-in, from the code to the signed-in device. Views read `snapshot()` and redraw on
 * `subscribe`, as with `SignInFlow`.
 */
export class QrLoginFlow {
  private view: QrLoginView = { type: 'starting' };
  private request: RequestId | null = null;
  private readonly listeners = new Set<() => void>();
  constructor(
    private readonly store: Store,
    private readonly start: QrLoginStart,
    /** Opens the account page the signed-in side approves on. */
    private readonly openUrl: (url: string) => void,
  ) {}

  snapshot = (): QrLoginView => this.view;

  subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);

    return () => {
      this.listeners.delete(listener);
    };
  };

  private set(view: QrLoginView): void {
    this.view = view;

    for (const listener of this.listeners) listener();
  }

  /** Whether this device can show a code. */
  get canShow(): boolean {
    return this.start.shown !== null;
  }

  /** Shows this device's code for the other one to scan. */
  show(): void {
    if (this.start.shown) this.begin(this.start.shown);
  }

  /** Opens the camera for the other device's code. */
  scan(): void {
    this.cancel();
    this.set({ type: 'scanning' });
  }

  /** The camera read the other device's code. */
  scanned(code: number[]): void {
    if (this.view.type === 'scanning') this.begin(this.start.scanned(code));
  }

  private begin(command: Command): void {
    this.cancel();

    const run = this.store.run(command, (event) => {
      if (this.request !== run.request) return;

      if (event.type === 'QrLogin') {
        // A check code sent stays sent until the next step.
        if (event.data.step.type !== 'EnterCheckCode' || this.view.type !== 'enterCheckCode')
          this.set(qrLoginView(event.data.step));
      } else if (event.type === 'OpenUrl') {
        this.openUrl(event.data.url);
      }
    });

    this.request = run.request;
    this.set({ type: 'starting' });
    run.then(
      () => this.finished(run.request),
      (cause: unknown) => this.failed(run.request, cause),
    );
  }

  /** Sends the two digits the other device shows; `false` when they aren't two digits. */
  enterCheckCode(text: string): boolean {
    const code = parseCheckCode(text);

    if (
      code === null ||
      this.request === null ||
      this.view.type !== 'enterCheckCode' ||
      this.view.sent
    )
      return false;
    answerRequest(this.store, this.request, { type: 'CheckCode', data: code });
    this.set({ type: 'enterCheckCode', sent: true });

    return true;
  }

  /** Opens the account page again, for a browser closed too early. */
  reopen(): void {
    if (this.view.type === 'approve') this.openUrl(this.view.url);
  }

  /**
   * Stops the exchange under way, if any (leaving the screen). One the engine reported done is
   * finishing, not under way: it is left to finish.
   */
  cancel(): void {
    const request = this.request;

    if (request === null || this.view.type === 'finishing') return;
    this.request = null;
    this.store.send({ type: 'Cancel', data: request }, { quiet: true });
  }

  private finished(request: RequestId): void {
    if (this.request !== request) return;
    this.request = null;
    // The account the engine said this device signed in as; none when approving another device.
    const added = this.view.type === 'finishing' ? this.view.account : null;

    if (added !== null) setActive(this.store, added);
    this.set({ type: 'done', account: added });
  }

  private failed(request: RequestId, cause: unknown): void {
    if (this.request !== request) return;
    this.request = null;

    // Cancelled by the server's side or the user: the screen says why it stopped all the same.
    const message =
      cause instanceof CommandFailed ? cause.message : cause instanceof Error ? cause.message : '';

    this.set({ type: 'failed', error: message });
  }
}
