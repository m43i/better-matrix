// What the create menu offers (the "+" at the bottom: Android's floating button, iOS's tab bar),
// as the desktop's create menu: a chat always; a room at Home or in a space the user may add
// rooms to; a space from Home; a section always.

import type { AccountData } from './types';

/** One thing the create menu makes. */
export type CreateChoice = 'chat' | 'room' | 'space' | 'section';

/** What the create menu offers for the account on screen, in its scope. */
export function createChoices(account: AccountData): CreateChoice[] {
  const scope = account.scope;
  const space = scope === 'home' ? null : scope.space;

  const canAddRooms =
    space === null || (account.spaces.find((s) => s.room_id === space)?.can_add_rooms ?? false);

  return [
    'chat',
    ...(canAddRooms ? (['room'] as const) : []),
    ...(space === null ? (['space'] as const) : []),
    'section',
  ];
}
