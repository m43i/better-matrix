// A new room (modal): its name, topic, the space it goes in (none, or one the user may add rooms
// to; the `space` param picks one ahead), who may join and its encryption (`NewRoomForm`). The
// new room replaces this modal.

import type { JoinRule } from '@better-matrix/matrix-native/types';
import { useLocalSearchParams, useRouter } from 'expo-router';

import { routes } from '../routes';
import { joinRuleParam } from '../state/admin';
import { create } from '../state/actions/people';
import { spacesForNewRoom } from '../state/people';
import { shallow, useAppStore, useStore } from '../state/react';
import { activeAccount } from '../state/selectors';
import { NewRoomForm } from '../ui/admin/NewRoomForm';

export default function NewRoomScreen() {
  const params = useLocalSearchParams<{ space?: string; join?: JoinRule }>();
  const store = useAppStore();
  const router = useRouter();
  const account = useStore((s) => s.active);
  const spaces = useStore((s) => spacesForNewRoom(activeAccount(s)?.spaces ?? []), shallow);

  if (account === null) return null;

  return (
    <NewRoomForm
      testID="new-room"
      account={account}
      space={false}
      spaces={spaces}
      initial={{ parent: params.space ?? null, joinRule: joinRuleParam(params.join) }}
      onCreate={async (command) => {
        const roomId = await create(store, command);
        router.replace(routes.room(account, roomId));
      }}
    />
  );
}
