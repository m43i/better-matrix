// Android's push task: defined when the bundle loads (index.ts imports this before the router),
// because a push may start the app's JavaScript headless, with no screen and no engine
// (expo-notifications' background task, run by expo-task-manager). iOS handles pushes in its
// Notification Service Extension instead.

import type { NotificationTaskPayload } from 'expo-notifications';
import * as TaskManager from 'expo-task-manager';
import { Platform } from 'react-native';

import { handlePush } from './push';
import { PUSH_TASK } from './pushRules';

if (Platform.OS === 'android') {
  TaskManager.defineTask<NotificationTaskPayload>(PUSH_TASK, async ({ data, error }) => {
    if (error) return;
    await handlePush(data).catch(() => undefined);
  });
}
