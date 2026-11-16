// Opens the permalinks the system hands the app (ui/links/inbox.ts) once there is somewhere to
// open them: an account signed in with its room list, and the router mounted. Signed out, they
// wait for the sign-in.

import { useRootNavigationState } from 'expo-router';
import { useEffect, useState } from 'react';

import { useAppStore, useStore } from '../../state/react';
import { onLinkDelivered, takeLinks } from './inbox';
import { openPermalink } from './open';

export function LinkOpener() {
  const store = useAppStore();
  const ready = useStore((s) => s.accounts.some((a) => a.roomsLoaded));
  const navigation = useRootNavigationState();
  const [delivered, setDelivered] = useState(0);
  useEffect(() => onLinkDelivered(() => setDelivered((count) => count + 1)), []);
  useEffect(() => {
    if (!ready || !navigation?.key) return;
    for (const link of takeLinks()) openPermalink(store, link);
  }, [ready, navigation?.key, delivered, store]);
  return null;
}
