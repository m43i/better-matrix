// A settings screen's hold on one room's settings: asks for them when it opens, shows them with
// the changes still on their way (`withPending`), and sends a change.

import type { AccountId, RoomSetting, RoomSettings } from '@better-matrix/matrix-native/types';
import { useCallback, useEffect, useMemo, useState } from 'react';

import { changeRoom, loadRoomSettings } from '../../state/actions/admin';
import { withPending } from '../../state/admin';
import { useAppStore, useStore } from '../../state/react';
import { rightsKey, roomSettings } from '../../state/selectors';

export interface RoomSettingsHold {
  /** The settings as shown, once loaded. */
  settings: RoomSettings | undefined;
  /** The settings as the room last confirmed them, without the changes on their way. */
  confirmed: RoomSettings | undefined;
  /** The changes on their way. */
  pending: readonly RoomSetting[];
  /** Sends one change; resolves once the room has it, rejects when it failed (told already). */
  change: (setting: RoomSetting) => Promise<void>;
}

export function useRoomSettings(account: AccountId, roomId: string): RoomSettingsHold {
  const store = useAppStore();
  const loaded = useStore((s) => roomSettings(s, account, roomId));
  const [pending, setPending] = useState<readonly RoomSetting[]>([]);

  // Read again whenever the room's rights as the room list sees them change (a power-level change
  // from elsewhere), so no control outlives the right behind it.
  const rights = useStore((s) => rightsKey(s, account, roomId));

  useEffect(() => loadRoomSettings(store, account, roomId), [store, account, roomId, rights]);

  const change = useCallback(
    (setting: RoomSetting) => {
      setPending((p) => [...p, setting]);

      return changeRoom(store, account, roomId, setting).finally(() =>
        setPending((p) => p.filter((x) => x !== setting)),
      );
    },
    [store, account, roomId],
  );

  const settings = useMemo(
    () => (loaded === undefined ? undefined : withPending(loaded, pending)),
    [loaded, pending],
  );

  return { settings, confirmed: loaded, pending, change };
}
