import type { RoomSummary } from '@better-matrix/matrix-native/types';

import { personName, roomName } from '../state/names';
import { useStore } from '../state/react';

export function usePersonName(name: string): string {
  return useStore((s) => personName(name, s.settings.reverseDisplayNames));
}

export function useRoomName(room: RoomSummary | undefined): string {
  return useStore((s) => (room ? roomName(room, s.settings.reverseDisplayNames) : ''));
}
