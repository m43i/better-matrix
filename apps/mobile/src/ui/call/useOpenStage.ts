// Back to the call's full stage, from the call bar, the room header or the compact stage over the
// chat: the stage and the chat show one at a time on a phone, so the chat closes.

import { useRouter } from 'expo-router';
import { useCallback } from 'react';

import { routes } from '../../routes';
import { showCallChat } from '../../state/actions/calls';
import { useAppStore } from '../../state/react';

export function useOpenStage(): () => void {
  const store = useAppStore();
  const router = useRouter();

  return useCallback(() => {
    showCallChat(store, false);
    router.push(routes.call());
  }, [store, router]);
}
