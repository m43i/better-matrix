// This build's push configuration (docs/push.md): the gateway from the environment
// (`EXPO_PUBLIC_PUSH_GATEWAY`, inlined at build time) and the EAS project from the app config
// (`extra.eas.projectId`, written by `eas init`). Nothing of it is in the code.

import Constants from 'expo-constants';
import { Platform } from 'react-native';

import { type PushConfig, type PushUnavailable, unavailable } from './pushRules';

export function pushConfig(): PushConfig {
  const eas = (Constants.expoConfig?.extra as { eas?: { projectId?: unknown } } | undefined)?.eas;
  return {
    gateway: process.env.EXPO_PUBLIC_PUSH_GATEWAY,
    projectId: typeof eas?.projectId === 'string' ? eas.projectId : undefined,
  };
}

/** Why push is off in this build, or null when it can run: the settings screen says so. */
export function pushUnavailable(): PushUnavailable | null {
  return Platform.OS === 'web' ? 'no-gateway' : unavailable(pushConfig());
}
