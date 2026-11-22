// A new space (modal): its name, topic and who may join (`NewRoomForm`). Once made, the account's
// room list shows it, on Home.

import type { JoinRule } from '@better-matrix/matrix-native/types';
import { useLocalSearchParams, useRouter } from 'expo-router';

import { routes } from '../routes';
import { joinRuleParam } from '../state/admin';
import { createSpace } from '../state/actions/people';
import { useAppStore, useStore } from '../state/react';
import { NewRoomForm } from '../ui/admin/NewRoomForm';

export default function NewSpaceScreen() {
  const params = useLocalSearchParams<{ join?: JoinRule }>();
  const store = useAppStore();
  const router = useRouter();
  const account = useStore((s) => s.active);

  if (account === null) return null;

  return (
    <NewRoomForm
      testID="new-space"
      account={account}
      space
      spaces={[]}
      initial={{ parent: null, joinRule: joinRuleParam(params.join) }}
      onCreate={async (command) => {
        await createSpace(store, command);
        router.dismissTo(routes.home());
      }}
    />
  );
}
