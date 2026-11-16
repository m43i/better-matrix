// System notifications for `Event::Notification`, unless its room is on screen while the app is
// in front, and opening the room when one is tapped. Permission is asked once an account is
// signed in (Android 13+ and iOS ask the user then), never before. Owned by the shell area.
// Pushes (push.ts) show through `presentContent` too, and each event shows once (`notified`).

import { router } from 'expo-router';
import * as Notifications from 'expo-notifications';
import { useEffect } from 'react';
import { AppState, Platform } from 'react-native';

import type { Notification } from '@better-matrix/matrix-native/types';
import { t } from '../i18n';
import { routes } from '../routes';
import { roomOnScreen } from '../state/focus';
import type { Store } from '../state/store';
import {
  contentOf,
  type NotificationContent,
  shouldPresent,
  type NotificationTarget,
  tapRoute,
  targetOf,
} from './notificationRules';
import { isPush, leftToPush, Seen } from './pushRules';

/** How long a tap waits for its room to be known (the first sync after launch) before opening it. */
const TAP_WAIT_MS = 15_000;

/** The Android channel messages notify on. */
const CHANNEL = 'messages';

/** The events notified about in this JavaScript context, by the sync or a push. */
export const notified = new Seen();

/** A room a tapped notification opens once the app can navigate (launched by the tap). */
let pending: NotificationTarget | null = null;

let canNavigate = false;

let asked = false;

/** The running app's store, which says whether a tap is about an invitation. */
let appStore: Store | null = null;

function open(target: NotificationTarget): void {
  if (!canNavigate || !appStore) {
    pending = target;

    return;
  }

  const store = appStore;

  // Opens once the account's lists say what the room is: an invitation or a conversation (after
  // `TAP_WAIT_MS` the room, whatever they say).
  const go = (waitedOut = false): boolean => {
    const route = tapRoute(store.getState(), target, waitedOut);

    if (route === 'wait') return false;

    if (route === 'invitation') router.push(routes.invitation(target.account, target.roomId));
    else router.push(routes.room(target.account, target.roomId, target.eventId));

    return true;
  };

  if (go()) return;

  const stop = store.subscribe(() => {
    if (go()) finish();
  });

  const timer = setTimeout(() => {
    go(true);
    finish();
  }, TAP_WAIT_MS);

  const finish = () => {
    stop();
    clearTimeout(timer);
  };
}

function opened(response: Notifications.NotificationResponse | null): void {
  if (!response) return;
  const target = targetOf(response.notification.request.content.data);

  if (!target) return;
  Notifications.clearLastNotificationResponse();
  open(target);
}

/** Asks to notify, once per run, while the system still lets the app ask. */
async function askPermission(): Promise<void> {
  if (asked) return;
  asked = true;
  // Android 13+ shows the prompt only once a channel exists.
  await ensureChannel();
  const current = await Notifications.getPermissionsAsync();

  if (current.granted || !current.canAskAgain) return;
  await Notifications.requestPermissionsAsync();
}

/** Android's channel for messages; creating it again changes nothing. */
async function ensureChannel(): Promise<void> {
  if (Platform.OS !== 'android') return;
  await Notifications.setNotificationChannelAsync(CHANNEL, {
    name: t('mobile.shell.channel_messages'),
    importance: Notifications.AndroidImportance.HIGH,
  });
}

async function present(n: Notification): Promise<void> {
  await presentContent(contentOf(n));
}

/** Shows a notification now (also from a push task with no screen). */
export async function presentContent(content: NotificationContent): Promise<void> {
  await ensureChannel();
  await Notifications.scheduleNotificationAsync({
    identifier: content.identifier,
    content: {
      title: content.title,
      body: content.body,
      data: content.data,
      threadIdentifier: content.thread,
      sound: 'default',
    },
    trigger: Platform.OS === 'android' ? { channelId: CHANNEL } : null,
  });
}

export function installNotifications(store: Store): void {
  if (Platform.OS === 'web') return;
  appStore = store;
  // What the app scheduled was already chosen to show (`shouldPresent`): show it, in front too.
  // A push reaching the app in front stays quiet: the running engine notifies itself.
  Notifications.setNotificationHandler({
    handleNotification: async (notification) => {
      const show = !isPush(notification.request.trigger);

      return {
        shouldShowBanner: show,
        shouldShowList: show,
        shouldPlaySound: show,
        shouldSetBadge: false,
      };
    },
  });
  store.addEffect((event) => {
    if (event.type === 'AccountAdded') {
      askPermission().catch(() => undefined);
    } else if (event.type === 'Notification') {
      const n = event.data;

      if (!shouldPresent(AppState.currentState === 'active', roomOnScreen(n.account, n.room_id)))
        return;
      const pushed = n.account in store.getState().pushers;

      if (leftToPush(Platform.OS === 'ios', AppState.currentState === 'active', pushed)) return;

      if (!notified.mark(n.account, n.event_id)) return;
      present(n).catch(() => undefined);
    }
  });
  Notifications.addNotificationResponseReceivedListener(opened);

  // The tap that launched the app.
  try {
    opened(Notifications.getLastNotificationResponse());
  } catch {
    // Nothing launched it.
  }
}

/**
 * Lets taps navigate while the calling screen is up, and opens the room of the tap that
 * launched the app. Home calls it: it is mounted once the app can show rooms.
 */
export function useNotificationOpener(): void {
  useEffect(() => {
    canNavigate = true;

    if (pending) {
      const target = pending;
      pending = null;
      open(target);
    }

    return () => {
      canNavigate = false;
    };
  }, []);
}
