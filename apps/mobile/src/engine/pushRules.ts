// What push decides, pure so it is tested without native modules (push.ts and pushTask.ts do
// what this decides): the gateway's URL, which accounts get a pusher, what changes when the
// token or a setting does, which account and event a push is for, and showing each event once.
// ADR 0010 records the design.

import { z } from 'zod';
import type {
  AccountId,
  Pusher,
  PushTarget,
  RoomSummary,
} from '@better-matrix/matrix-native/types';

import { wantsAttention } from '../state/sections';
import type { Settings } from '../state/settings';
import type { AccountData, PusherRegistration } from '../state/types';

/** The app ID every pusher of this app uses; the gateway accepts it (packages/push-gateway). */
export const PUSH_APP_ID = 'chat.bettermatrix.mobile';

/** The path the Matrix push gateway API notifies on. */
export const NOTIFY_PATH = '/_matrix/push/v1/notify';

/** The headless task Android runs a push in (expo-notifications' background task). */
export const PUSH_TASK = 'better-matrix-push';

/** What this build was configured with: none of it is in the code (docs/push.md). */
export interface PushConfig {
  /** The push gateway, from `EXPO_PUBLIC_PUSH_GATEWAY`. */
  gateway: string | undefined;
  /** The EAS project, from the app config (`extra.eas.projectId`). */
  projectId: string | undefined;
}

/** Why push is off in this build, if it is. */
export type PushUnavailable = 'no-gateway' | 'no-project';

/**
 * The gateway's notify URL from what was configured: a base URL gets the notify path; anything
 * that isn't an `https` URL (or `http` to a local test gateway) is no gateway.
 */
export function notifyUrl(gateway: string | undefined): string | null {
  const trimmed = gateway?.trim();

  if (!trimmed) return null;
  let url: URL;

  try {
    url = new URL(trimmed);
  } catch {
    return null;
  }

  const local =
    url.hostname === 'localhost' || url.hostname === '127.0.0.1' || url.hostname === '10.0.2.2';

  if (url.protocol !== 'https:' && !(url.protocol === 'http:' && local)) return null;
  const base = url.toString().replace(/\/+$/, '');

  return base.endsWith(NOTIFY_PATH) ? base : `${base}${NOTIFY_PATH}`;
}

/** Why push can't run with `config`, or null when it can. */
export function unavailable(config: PushConfig): PushUnavailable | null {
  if (!notifyUrl(config.gateway)) return 'no-gateway';

  if (!config.projectId?.trim()) return 'no-project';

  return null;
}

/** The accounts that should have a pusher on this device. */
export function wantedAccounts(
  accounts: readonly AccountId[],
  settings: Pick<Settings, 'pushEnabled' | 'pushOff'>,
): AccountId[] {
  if (!settings.pushEnabled) return [];

  return accounts.filter((account) => !settings.pushOff.includes(account));
}

/**
 * Whether a push for `account` may show: a pusher whose removal failed may still deliver after
 * push was turned off, for the device or the account.
 */
export function pushAllowed(
  account: AccountId,
  settings: Pick<Settings, 'pushEnabled' | 'pushOff'>,
): boolean {
  return wantedAccounts([account], settings).length > 0;
}

/** What to send so the registered pushers match what is wanted. */
export interface PushPlan {
  /** Accounts to register (or register again: a new token, gateway or language). */
  register: AccountId[];
  /** Accounts whose pusher to remove, with what it was registered with. */
  remove: Array<{ account: AccountId; registration: PusherRegistration }>;
}

function same(a: PusherRegistration, b: PusherRegistration): boolean {
  return a.token === b.token && a.gateway === b.gateway && a.lang === b.lang;
}

/**
 * The changes from `registered` to `wanted` with `current` (null: no token, so nothing new is
 * registered). A changed token also removes the pusher of the old one.
 */
export function plan(
  registered: Readonly<Record<AccountId, PusherRegistration>>,
  wanted: readonly AccountId[],
  current: PusherRegistration | null,
): PushPlan {
  const result: PushPlan = { register: [], remove: [] };

  for (const [account, registration] of Object.entries(registered)) {
    const keep =
      wanted.includes(account) && current !== null && registration.token === current.token;

    if (!keep && (!wanted.includes(account) || current !== null))
      result.remove.push({ account, registration });
  }

  if (current) {
    for (const account of wanted) {
      const before = registered[account];

      if (!before || !same(before, current)) result.register.push(account);
    }
  }

  return result;
}

/** What carrying out a `PushPlan` does, given by the caller (the engine and the saved list). */
export interface PlanSteps {
  /** Whether the account is signed in here: a signed-out account's pushers went with its device. */
  signedIn(account: AccountId): boolean;
  /** Removes the pusher on the server; rejects when that failed. */
  remove(account: AccountId, registration: PusherRegistration): Promise<void>;
  /** Registers the current pusher on the server; rejects when that failed. */
  register(account: AccountId): Promise<void>;
  /** The account's pusher is gone: forget it here. */
  removed(account: AccountId): void;
  /** The current pusher is registered: remember it. */
  registered(account: AccountId): void;
  /** Registering failed. */
  failed(account: AccountId, cause: unknown): void;
}

/**
 * Carries `changes` out; false when a step failed, for the caller to try again later. A pusher
 * whose removal failed stays remembered, and its account isn't registered again in the same
 * pass, so a new token can't take its place and leave the old pusher behind.
 */
export async function carryOut(changes: PushPlan, steps: PlanSteps): Promise<boolean> {
  let settled = true;
  const blocked = new Set<AccountId>();

  for (const { account, registration } of changes.remove) {
    if (steps.signedIn(account)) {
      try {
        await steps.remove(account, registration);
      } catch {
        settled = false;
        blocked.add(account);
        continue;
      }
    }

    steps.removed(account);
  }

  for (const account of changes.register) {
    if (blocked.has(account)) continue;

    try {
      await steps.register(account);
      steps.registered(account);
    } catch (error) {
      settled = false;
      steps.failed(account, error);
    }
  }

  return settled;
}

/** Waits between failed asks for the push token: from 30 seconds, doubling, to 10 minutes. */
export function tokenRetryMs(failures: number): number {
  return Math.min(30_000 * 2 ** Math.max(0, failures - 1), 600_000);
}

/**
 * Asks for the push token (`get`) one ask at a time, handing each new one to `got`. A failed ask
 * (offline at launch) is tried again later (`tokenRetryMs`) while `wanted` says the token is
 * still needed. The returned function asks now.
 */
export function tokenFetcher(options: {
  get: () => Promise<string>;
  got: (token: string) => void;
  wanted: () => boolean;
}): () => void {
  let fetching = false;
  let failures = 0;
  let timer: ReturnType<typeof setTimeout> | null = null;

  const fetch = () => {
    if (fetching) return;
    fetching = true;
    options
      .get()
      .then((token) => {
        failures = 0;
        options.got(token);
      })
      .catch(() => {
        failures += 1;

        if (timer) return;
        timer = setTimeout(() => {
          timer = null;

          if (options.wanted()) fetch();
        }, tokenRetryMs(failures));
      })
      .finally(() => {
        fetching = false;
      });
  };

  return fetch;
}

/** What iOS shows until the Notification Service Extension replaced it (it may not get to). */
export interface FallbackText {
  title: string;
  body: string;
}

/**
 * The pusher for `account`. iOS pushes carry a fallback text and are mutable, for the
 * Notification Service Extension; Android's are data only, for the push task.
 */
export function pusherOf(
  registration: PusherRegistration,
  platform: 'ios' | 'android',
  deviceName: string,
  appName: string,
  fallback: FallbackText,
): Pusher {
  return {
    app_id: PUSH_APP_ID,
    push_key: registration.token,
    gateway_url: registration.gateway,
    app_display_name: appName,
    device_display_name: deviceName,
    lang: registration.lang,
    payload: platform === 'ios' ? JSON.stringify({ alert: fallback }) : null,
  };
}

/** The event identity sent by our gateway; other fields are never used as message content. */
const pushIdentity = z.object({
  account: z.string().min(1),
  room_id: z.string().min(1),
  event_id: z.string().min(1),
  unread: z.json().optional(),
  unread_count: z.json().optional(),
});

const unreadCount = z
  .union([z.number(), z.string().regex(/^\d+$/).transform(Number)])
  .nullable()
  .catch(null);

const pushTarget = pushIdentity.transform((data): PushTarget => ({
  account: data.account,
  room_id: data.room_id,
  event_id: data.event_id,
  unread: unreadCount.parse(data.unread ?? data.unread_count),
}));

const pushJson = z
  .string()
  .transform((json, context) => {
    try {
      return JSON.parse(json);
    } catch {
      context.addIssue({ code: 'custom', message: 'Invalid push JSON' });

      return z.NEVER;
    }
  })
  .pipe(pushTarget);

/** A gateway push, flat or wrapped by Expo as an object or JSON string. */
export const targetOfPush = z
  .union([
    pushTarget,
    z.object({ body: z.union([pushTarget, pushJson]) }).transform(({ body }) => body),
    z.object({ dataString: pushJson }).transform(({ dataString }) => dataString),
  ])
  .nullable()
  .catch(null).parse;

/** Whether a system notification came as a push rather than one the app scheduled. */
export const isPush = z
  .object({ type: z.literal('push') })
  .transform(() => true)
  .catch(false).parse;

/**
 * The events notified about already, so the engine's sync and a push never show one twice.
 * Remembers the latest `limit`.
 */
export class Seen {
  private readonly keys: string[] = [];
  private readonly set = new Set<string>();

  constructor(private readonly limit = 500) {}

  /** Marks the event seen; false when it was already. */
  mark(account: AccountId, eventId: string): boolean {
    const key = `${account}|${eventId}`;

    if (this.set.has(key)) return false;
    this.set.add(key);
    this.keys.push(key);

    if (this.keys.length > this.limit) {
      const oldest = this.keys.shift();

      if (oldest !== undefined) this.set.delete(oldest);
    }

    return true;
  }
}

/**
 * Whether the engine's own notification should give way to the push for the same event: on iOS
 * in the background, where the Notification Service Extension (another process, which can't be
 * asked) shows every push of an account with a pusher. Android's push task runs in the app's
 * JavaScript and shows each event once (`Seen`).
 */
export function leftToPush(ios: boolean, foreground: boolean, pushed: boolean): boolean {
  return ios && !foreground && pushed;
}

/** How often the app, in front, tells the Notification Service Extension so (`app-active`). */
export const ACTIVE_EVERY_MS = 30_000;

/** How many rooms of a list want attention, per list (lists are replaced, never changed). */
const attention = new WeakMap<readonly RoomSummary[], number>();

function attentionIn(rooms: readonly RoomSummary[]): number {
  let count = attention.get(rooms);

  if (count === undefined) {
    count = rooms.filter(wantsAttention).length;
    attention.set(rooms, count);
  }

  return count;
}

/** The app icon's badge: how many conversations want attention, over every account. */
export function badgeCount(accounts: readonly AccountData[]): number {
  return accounts.reduce((sum, account) => sum + attentionIn(account.rooms), 0);
}

/** What the app writes for the Notification Service Extension to word its notifications. */
export interface PushStrings {
  app_name: string;
  preview: string;
  hidden: string;
  invite: string;
}
