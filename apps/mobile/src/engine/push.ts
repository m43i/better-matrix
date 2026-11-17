// Push notifications (ADR 0010): this device's pusher on each account, kept in step with the
// accounts, the settings and the push token; pushes turned into notifications while the app
// isn't in front; and what the iOS Notification Service Extension needs from the app. What it
// decides is in pushRules.ts. Without a configured gateway or EAS project, push stays off and
// notifications come only from the running engine.
//
// The homeserver sends the gateway (packages/push-gateway) only room and event IDs; the gateway
// hands them to Expo's push service with the account. Android runs `handlePush` in the app's
// JavaScript (pushTask.ts, headless when the app isn't running); iOS's extension does the same
// natively with the engine's library.

import { z } from 'zod';
import { fetchPushWithoutEngine } from '@better-matrix/matrix-native/native';
import type { AccountId, PushContent, PushTarget } from '@better-matrix/matrix-native/types';
import * as Device from 'expo-device';
import { type Directory, File } from 'expo-file-system';
import * as Notifications from 'expo-notifications';
import { AppState, Platform } from 'react-native';

import { locale, setLanguage, t } from '../i18n';
import { addNotice } from '../state/reducer';
import type { Store } from '../state/store';
import type { PusherRegistration, State } from '../state/types';
import { pushContentOf } from './notificationRules';
import { notified, presentContent } from './notifications';
import type { ForegroundMarker } from './front';
import { dataDirectory, pathOf, readSettings } from './paths';
import { pushConfig } from './pushConfig';
import {
  ACTIVE_EVERY_MS,
  badgeCount,
  notifyUrl,
  PUSH_APP_ID,
  PUSH_TASK,
  type PushStrings,
  carryOut,
  plan,
  pushAllowed,
  tokenFetcher,
  pusherOf,
  targetOfPush,
  unavailable,
  wantedAccounts,
} from './pushRules';
import { secureStore } from './secrets';

/** The store of the running app in this JavaScript context, once it booted. */
let running: Store | null = null;

const registrationsFile = (data: Directory) => new File(data, 'pushers.json');

function readRegistrations(data: Directory): Record<AccountId, PusherRegistration> {
  try {
    const file = registrationsFile(data);

    const parsed = z
      .record(
        z.string(),
        z.object({
          token: z.string(),
          gateway: z.string(),
          lang: z.string(),
        }),
      )
      .safeParse(file.exists ? JSON.parse(file.textSync()) : {});

    return parsed.success ? parsed.data : {};
  } catch {
    return {};
  }
}

function writeRegistrations(data: Directory, pushers: State['pushers']): void {
  try {
    registrationsFile(data).write(JSON.stringify(pushers));
  } catch {
    // Written again with the next change.
  }
}

/** The texts the Notification Service Extension words notifications with, in the app's language. */
function pushStrings(): PushStrings {
  return {
    app_name: t('app.name'),
    preview: t('overview.preview', { sender: '{{sender}}', body: '{{body}}' }),
    hidden: t('mobile.push.new_message'),
    invite: t('mobile.push.invite', { sender: '{{sender}}' }),
  };
}

/**
 * Starts push for the running app: keeps pushers registered as accounts, settings and the token
 * change, keeps the badge, and tells the iOS extension how to word notifications. `data` is the
 * engine's data directory. Returns what tells the extension the app is in front, on iOS with
 * push on.
 */
export function installPush(store: Store, data: Directory): ForegroundMarker | undefined {
  if (Platform.OS === 'web') return undefined;
  running = store;
  store.update((s) => ({ ...s, pushers: readRegistrations(data) }));
  installBadge(store);
  const config = pushConfig();
  const off = unavailable(config);

  if (off) {
    if (__DEV__) console.warn(`push is off in this build (${off}): see docs/push.md`);

    return undefined;
  }

  const gateway = notifyUrl(config.gateway) ?? '';
  const projectId = config.projectId ?? '';

  const marker = Platform.OS === 'android' ? undefined : foregroundMarker(data);

  if (Platform.OS === 'android') Notifications.registerTaskAsync(PUSH_TASK).catch(() => undefined);

  let token: string | null = null;
  let queue = Promise.resolve();
  let retry: ReturnType<typeof setTimeout> | null = null;

  /**
   * Brings the pushers in step; one pass at a time, in order, again a minute later while a step
   * fails (quietly: the first failure said so).
   */
  const sync = (quiet = false) => {
    queue = queue
      .then(() => syncPushers(store, data, token, gateway, quiet))
      .then((settled) => {
        if (settled || retry) return;
        retry = setTimeout(() => {
          retry = null;
          sync(true);
        }, RETRY_PUSHERS_MS);
      })
      .catch(() => undefined);
  };

  const fetchToken = tokenFetcher({
    get: () =>
      Notifications.getExpoPushTokenAsync({ projectId }).then(
        (expo) => expo.data,
        (cause: unknown) => {
          if (__DEV__) console.warn('no push token:', cause);
          throw cause;
        },
      ),
    got: (fresh) => {
      if (fresh === token) return;
      token = fresh;
      sync();
    },
    // Offline at launch: asked again while an account needs it.
    wanted: () => token === null && store.getState().accounts.length > 0,
  });

  // The device's FCM or APNs token changed: Expo's token for it may have too.
  Notifications.addPushTokenListener(() => fetchToken());

  let watched = { accounts: '', enabled: true, off: '', language: '' };
  store.subscribe(() => {
    const s = store.getState();

    const now = {
      accounts: s.accounts.map((a) => a.summary.id).join('\n'),
      enabled: s.settings.pushEnabled,
      off: s.settings.pushOff.join('\n'),
      language: s.settings.language,
    };

    if (
      now.accounts === watched.accounts &&
      now.enabled === watched.enabled &&
      now.off === watched.off &&
      now.language === watched.language
    ) {
      return;
    }

    const languageChanged = now.language !== watched.language;
    watched = now;

    if (languageChanged && Platform.OS === 'ios') writeStrings(data);
    sync();

    // The token is asked for once there is an account to push to (the permission prompt
    // comes with the first account, notifications.ts), and again while it is missing.
    if (token === null && now.accounts !== '') fetchToken();
  });

  if (Platform.OS === 'ios') writeStrings(data);

  return marker;
}

function writeStrings(data: Directory): void {
  try {
    new File(data, 'push-strings.json').write(JSON.stringify(pushStrings()));
  } catch {
    // English stands in.
  }
}

/** How long until pushers that failed to change are tried again. */
const RETRY_PUSHERS_MS = 60_000;

/**
 * One pass: registers and removes pushers until they match what is wanted. False when a step
 * failed (`carryOut`): it is tried again later, and pushes a pusher that should be gone still
 * delivers are dropped (`pushAllowed`). A failed registration says so, unless `quiet` (a retry).
 */
async function syncPushers(
  store: Store,
  data: Directory,
  token: string | null,
  gateway: string,
  quiet: boolean,
): Promise<boolean> {
  const state = store.getState();
  const accounts = state.accounts.map((a) => a.summary.id);
  const wanted = wantedAccounts(accounts, state.settings);
  const current: PusherRegistration | null = token ? { token, gateway, lang: locale() } : null;

  const save = (change: (pushers: State['pushers']) => State['pushers']) => {
    store.update((s) => ({ ...s, pushers: change(s.pushers) }));
    writeRegistrations(data, store.getState().pushers);
  };

  const deviceName = Device.deviceName ?? Device.modelName ?? t('app.name');
  const fallback = { title: t('app.name'), body: t('mobile.push.new_message') };
  const platform = Platform.OS === 'ios' ? 'ios' : 'android';

  return carryOut(plan(state.pushers, wanted, current), {
    signedIn: (account) => accounts.includes(account),
    remove: (account, registration) =>
      store.run({
        type: 'RemovePusher',
        data: { account, app_id: PUSH_APP_ID, push_key: registration.token },
      }),
    register: (account) =>
      current
        ? store.run({
            type: 'RegisterPusher',
            data: {
              account,
              pusher: pusherOf(current, platform, deviceName, t('app.name'), fallback),
            },
          })
        : Promise.resolve(),
    removed: (account) => save(({ [account]: _removed, ...rest }) => rest),
    registered: (account) => current && save((pushers) => ({ ...pushers, [account]: current })),
    failed: (account, error) => {
      if (quiet) return;
      const reason = error instanceof Error ? error.message : String(error);
      store.update((s) => addNotice(s, t('mobile.push.failed', { account, error: reason })));
    },
  });
}

/** Keeps the app icon's badge at the conversations wanting attention. */
function installBadge(store: Store): void {
  let shown = -1;
  store.subscribe(() => {
    const count = badgeCount(store.getState().accounts);

    if (count === shown) return;
    shown = count;
    Notifications.setBadgeCountAsync(count).catch(() => undefined);
  });
}

/**
 * Tells the iOS extension the app is in front (`app-active`, refreshed every 30 s): it then
 * leaves pushes alone, since the running engine notifies itself and two processes would race
 * on the store. `followFront` shows and hides it.
 */
function foregroundMarker(data: Directory): ForegroundMarker {
  const marker = new File(data, 'app-active');
  let timer: ReturnType<typeof setInterval> | null = null;

  const write = () => {
    try {
      marker.write(String(Date.now()));
    } catch {
      // The extension then handles pushes as if in the background, which is safe.
    }
  };

  return {
    show() {
      write();
      timer ??= setInterval(write, ACTIVE_EVERY_MS);
    },
    hide() {
      if (timer) clearInterval(timer);
      timer = null;

      try {
        if (marker.exists) marker.delete();
      } catch {
        // It goes stale within a minute anyway.
      }
    },
  };
}

function hiddenOf(target: PushTarget): PushContent {
  return {
    type: 'Hidden',
    data: {
      account: target.account,
      room_id: target.room_id,
      room_name: null,
      event_id: target.event_id,
    },
  };
}

/** Asks the running engine what a push is about. */
async function fetchThroughEngine(store: Store, target: PushTarget): Promise<PushContent> {
  let content = hiddenOf(target);

  try {
    await store.run(
      {
        type: 'FetchNotification',
        data: { account: target.account, room_id: target.room_id, event_id: target.event_id },
      },
      (event) => {
        if (event.type === 'PushContent') content = event.data.content;
      },
    );
  } catch {
    // Not signed in yet (still restoring), or the server failed: something generic shows.
  }

  return content;
}

/**
 * Handles one push the system handed the app's JavaScript (Android's push task): fetches and
 * decrypts its event, through the running engine or, with none, from the account's store, and
 * shows it once. In front, the running engine notifies itself and the push is left alone.
 */
export async function handlePush(payload: Notifications.NotificationTaskPayload): Promise<void> {
  if ('actionIdentifier' in payload) return;
  const target = targetOfPush(payload.data);

  if (!target) return;
  const store = running;

  if (store && AppState.currentState === 'active') return;
  // Headless: nothing booted, so the settings (and the language) come from the saved file.
  const settings = store ? store.getState().settings : readSettings();

  if (!pushAllowed(target.account, settings)) return;
  let content: PushContent;

  if (store) {
    content = await fetchThroughEngine(store, target);
  } else {
    setLanguage(settings.language);
    content = await fetchPushWithoutEngine(pathOf(dataDirectory().uri), target, secureStore).catch(
      () => hiddenOf(target),
    );
  }

  const shown = pushContentOf(content);

  if (!shown || !notified.mark(target.account, target.event_id)) return;
  await presentContent(shown);
}
