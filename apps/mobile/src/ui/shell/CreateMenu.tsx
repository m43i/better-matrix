// Home's header buttons: the bell (Activity, with how many notifications are new), then "+", a
// native menu of what can be made here, as the desktop's create menu: a chat; a room (in a space
// the user may add rooms to, or at Home); a space from Home; a section always.

import type { AccountId } from '@better-matrix/matrix-native/types';
import { router, Stack } from 'expo-router';

import { t } from '../../i18n';
import { routes } from '../../routes';
import { useStore } from '../../state/react';
import { account as accountOf } from '../../state/selectors';
import type { Scope } from '../../state/types';
import { usePalette } from '../theme';
import { useToolbarIcon } from './toolbarIcon';

export function CreateMenu({ account, scope }: { account: AccountId; scope: Scope }) {
  const palette = usePalette();
  const plus = useToolbarIcon('plus', palette.text);
  const bell = useToolbarIcon('bell', palette.text);
  const unseen = useStore((s) => s.activityUnseen);
  const chat = useToolbarIcon('compose', palette.text);
  const room = useToolbarIcon('hash', palette.text);
  const space = useToolbarIcon('space', palette.text);
  const section = useToolbarIcon('bullets', palette.text);
  const spaceId = typeof scope === 'object' ? scope.space : null;
  const canAddRooms = useStore((s) =>
    spaceId === null ? true : (accountOf(s, account)?.spaces.find((x) => x.room_id === spaceId)?.can_add_rooms ?? false),
  );
  return (
    <Stack.Toolbar placement="right">
      <Stack.Toolbar.Button icon={bell} accessibilityLabel={t('mobile.tabs.activity')} onPress={() => router.push(routes.activity())}>
        {unseen > 0 ? <Stack.Toolbar.Badge>{unseen > 99 ? '99+' : String(unseen)}</Stack.Toolbar.Badge> : null}
      </Stack.Toolbar.Button>
      <Stack.Toolbar.Menu icon={plus} accessibilityLabel={t('create.menu')}>
        <Stack.Toolbar.MenuAction icon={chat} onPress={() => router.push(routes.newChat())}>
          {t('create.chat')}
        </Stack.Toolbar.MenuAction>
        <Stack.Toolbar.MenuAction
          icon={room}
          hidden={!canAddRooms}
          onPress={() => router.push(routes.newRoom(spaceId ?? undefined))}
        >
          {t('create.room')}
        </Stack.Toolbar.MenuAction>
        <Stack.Toolbar.MenuAction icon={space} hidden={spaceId !== null} onPress={() => router.push(routes.newSpace())}>
          {t('create.space')}
        </Stack.Toolbar.MenuAction>
        <Stack.Toolbar.MenuAction icon={section} onPress={() => router.push(routes.sectionMenu(account))}>
          {t('sections.new')}
        </Stack.Toolbar.MenuAction>
      </Stack.Toolbar.Menu>
    </Stack.Toolbar>
  );
}
