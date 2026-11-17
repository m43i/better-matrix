// The create menu's items as every form of it draws them (iOS's create sheet, Android's floating
// button menu): each choice's icon, label and the screen it opens.

import type { Href } from 'expo-router';

import type { Key } from '../../i18n';
import { routes } from '../../routes';
import type { CreateChoice } from '../../state/create';
import type { AccountData } from '../../state/types';
import type { IconName } from '../icons';

export const CREATE_ITEMS: Record<CreateChoice, { icon: IconName; label: Key }> = {
  chat: { icon: 'compose', label: 'create.chat' },
  room: { icon: 'hash', label: 'create.room' },
  space: { icon: 'space', label: 'create.space' },
  section: { icon: 'bullets', label: 'sections.new' },
};

/** The screen `choice` opens, for `account` in its scope (a room is made in the space shown). */
export function createHref(choice: CreateChoice, account: AccountData): Href {
  switch (choice) {
    case 'chat':
      return routes.newChat();
    case 'room':
      return routes.newRoom(account.scope === 'home' ? undefined : account.scope.space);
    case 'space':
      return routes.newSpace();
    case 'section':
      return routes.sectionMenu(account.summary.id);
  }
}

/**
 * How the create menu sits in iOS's tab bar: on its own surface beside the tabs (iOS 27's
 * prominent tab), in the search tab's separated place (iOS 26.1), or as one more tab before
 * that. Android has no create tab: its floating button opens the menu.
 */
export type CreateTab = 'prominent' | 'separate' | 'tab' | 'none';

export function createTab(os: string, version: string | number): CreateTab {
  if (os !== 'ios') return 'none';

  const [major = 0, minor = 0] = String(version)
    .split('.')
    .map((part) => Number.parseInt(part, 10) || 0);

  if (major >= 27) return 'prominent';

  if (major === 26 && minor >= 1) return 'separate';

  return 'tab';
}
