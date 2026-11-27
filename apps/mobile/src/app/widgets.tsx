// The room's Integrations screen, opened from Room info.

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
import { Button, Divider, EmptyState, ListRow, ListSection } from '../ui/components';
import { useToolbarIcon } from '../ui/shell/toolbarIcon';
import { headerTint, usePalette } from '../ui/theme';
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
  const failed = useStore((s) => accountOf(s, account)?.widgetErrors[roomId] ?? false);

  useEffect(() => listWidgets(store, account, roomId), [store, account, roomId]);

  const open = (widget: Widget) =>
    openWidget(router, account, roomId, { type: 'widget', widgetId: widget.id });

  const inBrowser = (widget: Widget) =>
    openWidget(router, account, roomId, { type: 'widget', widgetId: widget.id, browser: true });

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

  const permissions = (widget: Widget) =>
    openWidget(router, account, roomId, { type: 'permissions', widgetId: widget.id });

  const actions = (widget: Widget) => {
    if (Platform.OS === 'ios') {
      const options = [
        t('mobile.widgets.open'),
        t('mobile.widgets.open_browser'),
        t('widgets.manage_permissions'),
        ...(widget.can_remove ? [t('widgets.remove')] : []),
        t('common.cancel'),
      ];

      ActionSheetIOS.showActionSheetWithOptions(
        {
          title: widget.name,
          message: widget.host,
          options,
          cancelButtonIndex: options.length - 1,
          destructiveButtonIndex: widget.can_remove ? 3 : undefined,
        },
        (index) => {
          if (index === 0) open(widget);
          else if (index === 1) inBrowser(widget);
          else if (index === 2) permissions(widget);
          else if (index === 3 && widget.can_remove) askToRemove(widget);
        },
      );

      return;
    }

    const browserActions = () =>
      Alert.alert(widget.name, widget.host, [
        { text: t('common.cancel'), style: 'cancel' },
        ...(widget.can_remove
          ? [{ text: t('widgets.remove'), onPress: () => askToRemove(widget) }]
          : []),
        { text: t('mobile.widgets.open_browser'), onPress: () => inBrowser(widget) },
      ]);

    Alert.alert(widget.name, widget.host, [
      { text: t('common.cancel'), style: 'cancel' },
      { text: t('widgets.manage_permissions'), onPress: () => permissions(widget) },
      ...(widget.can_remove
        ? [{ text: t('widgets.more'), onPress: browserActions }]
        : [{ text: t('mobile.widgets.open_browser'), onPress: () => inBrowser(widget) }]),
    ]);
  };

  return (
    <>
      <Stack.Screen options={{ title: t('widgets.integrations') }} />
      {canStart || listed?.can_add ? (
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
          </Stack.Toolbar.Menu>
        </Stack.Toolbar>
      ) : null}
      <FlatList<Widget>
        testID="widgets"
        contentInsetAdjustmentBehavior="automatic"
        data={[...meetings, ...others]}
        keyExtractor={(widget) => widget.id}
        ItemSeparatorComponent={() => <Divider inset={52} />}
        ListHeaderComponent={
          failed ? (
            <>
              <EmptyState icon="alert" title={t('widgets.load_failed')} />
              <Button
                title={t('common.try_again')}
                testID="widgets-retry"
                kind="secondary"
                onPress={() => listWidgets(store, account, roomId)}
              />
            </>
          ) : undefined
        }
        ListEmptyComponent={
          failed ? undefined : listed === undefined ? (
            <EmptyState loading title={t('info.loading')} />
          ) : (
            <EmptyState icon="widgets" title={t('widgets.none')} />
          )
        }
        ListFooterComponent={
          <>
            {(listed?.services ?? []).length ? (
              <ListSection title={t('widgets.services')}>
                {listed?.services?.map((userId) => (
                  <ListRow
                    key={userId}
                    icon="package"
                    title={userId}
                    onPress={() => router.push(routes.member(account, roomId, userId))}
                  />
                ))}
              </ListSection>
            ) : null}
            {(listed?.bridges ?? []).length ? (
              <ListSection title={t('widgets.bridges')}>
                {listed?.bridges?.map((bridge) => (
                  <ListRow
                    key={bridge.id}
                    icon="package"
                    title={bridge.protocol}
                    subtitle={
                      bridge.network ? [bridge.network, bridge.channel].join(' · ') : bridge.channel
                    }
                  />
                ))}
              </ListSection>
            ) : null}
            <ListSection>
              <ListRow
                testID="widget-integrations"
                icon="package"
                title={t('widgets.integrations_open')}
                onPress={() => openWidget(router, account, roomId, { type: 'integrations' })}
              />
            </ListSection>
          </>
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
