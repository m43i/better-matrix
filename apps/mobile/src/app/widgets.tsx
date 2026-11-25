// A room's widgets (from its details): web apps pinned to the room, in one list, its meetings
// first. A tap runs one in the app (the widget screen; a meeting is joined); a long press offers
// Open, Open in browser and, for whoever may, Remove (with a confirmation). The header's + is a
// native menu: Start a meeting (where the room can start one, `can_start_meeting`), Add widget
// (for whoever may add widgets) and Bots and integrations (the integration manager).

import type { AccountId, Widget } from '@better-matrix/matrix-native/types';
import { Stack, useLocalSearchParams, useRouter } from 'expo-router';
import { useEffect } from 'react';
import { ActionSheetIOS, Alert, FlatList, Platform } from 'react-native';

import { t } from '../i18n';
import { routes } from '../routes';
import { listWidgets, removeWidget } from '../state/actions/developer';
import { useAppStore, useStore } from '../state/react';
import { account as accountOf, canStartMeeting } from '../state/selectors';
import { widgetSections } from '../state/widgets';
import { Divider, EmptyState, ListRow } from '../ui/components';
import { useToolbarIcon } from '../ui/shell/toolbarIcon';
import { headerTint, usePalette } from '../ui/theme';
import { openWidgetInSystemBrowser } from '../ui/widgets/browser';
import { openWidget } from '../ui/widgets/launch';

export default function WidgetsScreen() {
  const { account, roomId } = useLocalSearchParams<{ account: AccountId; roomId: string }>();
  const store = useAppStore();
  const router = useRouter();
  const palette = usePalette();
  const listed = useStore((s) => accountOf(s, account)?.widgets[roomId]);
  const canStart = useStore((s) => canStartMeeting(s, account, roomId));
  const { meetings, others } = widgetSections(listed);
  const plus = useToolbarIcon('plus', headerTint(palette));
  const meetingIcon = useToolbarIcon('meeting', palette.text);
  const widgetIcon = useToolbarIcon('widgets', palette.text);
  const botIcon = useToolbarIcon('package', palette.text);

  useEffect(() => listWidgets(store, account, roomId), [store, account, roomId]);

  const open = (widget: Widget) =>
    openWidget(router, account, roomId, { type: 'widget', widgetId: widget.id });

  const inBrowser = (widget: Widget) =>
    openWidgetInSystemBrowser(store, account, roomId, widget.id);

  const askToRemove = (widget: Widget) =>
    Alert.alert(
      t('confirm.remove_widget_title', { name: widget.name }),
      t('confirm.remove_widget_body'),
      [
        { text: t('common.cancel'), style: 'cancel' },
        {
          text: t('widgets.remove'),
          style: 'destructive',
          onPress: () => removeWidget(store, account, roomId, widget.id),
        },
      ],
    );

  const actions = (widget: Widget) => {
    if (Platform.OS === 'ios') {
      const options = [
        t('mobile.widgets.open'),
        t('mobile.widgets.open_browser'),
        ...(widget.can_remove ? [t('widgets.remove')] : []),
        t('common.cancel'),
      ];

      ActionSheetIOS.showActionSheetWithOptions(
        {
          title: widget.name,
          message: widget.host,
          options,
          cancelButtonIndex: options.length - 1,
          destructiveButtonIndex: widget.can_remove ? 2 : undefined,
        },
        (index) => {
          if (index === 0) open(widget);
          else if (index === 1) inBrowser(widget);
          else if (index === 2 && widget.can_remove) askToRemove(widget);
        },
      );

      return;
    }

    // Android's alert holds three buttons: a tap already opens the widget here.
    Alert.alert(widget.name, widget.host, [
      { text: t('common.cancel'), style: 'cancel' },
      ...(widget.can_remove
        ? [{ text: t('widgets.remove'), onPress: () => askToRemove(widget) }]
        : [{ text: t('mobile.widgets.open'), onPress: () => open(widget) }]),
      { text: t('mobile.widgets.open_browser'), onPress: () => inBrowser(widget) },
    ]);
  };

  return (
    <>
      <Stack.Screen options={{ title: t('widgets.title') }} />
      <Stack.Toolbar placement="right">
        <Stack.Toolbar.Menu icon={plus} accessibilityLabel={t('widgets.add')}>
          <Stack.Toolbar.MenuAction
            icon={meetingIcon}
            hidden={!canStart}
            onPress={() => openWidget(router, account, roomId, { type: 'meeting', video: true })}
          >
            {t('mobile.widgets.start_meeting')}
          </Stack.Toolbar.MenuAction>
          <Stack.Toolbar.MenuAction
            icon={widgetIcon}
            hidden={!listed?.can_add}
            onPress={() => router.push(routes.addWidget(account, roomId))}
          >
            {t('widgets.add')}
          </Stack.Toolbar.MenuAction>
          <Stack.Toolbar.MenuAction
            icon={botIcon}
            onPress={() => openWidget(router, account, roomId, { type: 'integrations' })}
          >
            {t('mobile.widgets.integrations')}
          </Stack.Toolbar.MenuAction>
        </Stack.Toolbar.Menu>
      </Stack.Toolbar>
      <FlatList<Widget>
        testID="widgets"
        contentInsetAdjustmentBehavior="automatic"
        data={[...meetings, ...others]}
        keyExtractor={(widget) => widget.id}
        ItemSeparatorComponent={() => <Divider inset={52} />}
        ListEmptyComponent={
          listed === undefined ? (
            <EmptyState loading title={t('info.loading')} />
          ) : (
            <EmptyState icon="widgets" title={t('widgets.none')} />
          )
        }
        renderItem={({ item }) => (
          <ListRow
            testID={`widget-${item.id}`}
            icon={item.meeting === null ? 'widgets' : 'video'}
            title={item.name}
            subtitle={item.host}
            value={item.meeting === null ? undefined : t('call.join')}
            onPress={() => open(item)}
            onLongPress={() => actions(item)}
          />
        )}
      />
    </>
  );
}
