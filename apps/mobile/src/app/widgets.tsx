// A room's widgets (from its details): web apps pinned to the room, each opened in the browser.
// Whoever may change the room's widgets adds them from the header and removes them with a
// confirmation.

import type { Widget } from '@better-matrix/matrix-native/types';
import { Stack, useLocalSearchParams, useRouter } from 'expo-router';
import { useEffect } from 'react';
import { Alert, FlatList, Linking, Pressable, StyleSheet, Text } from 'react-native';

import { t } from '../i18n';
import { routes } from '../routes';
import { listWidgets, removeWidget } from '../state/actions/developer';
import { useAppStore, useStore } from '../state/react';
import { account as accountOf } from '../state/selectors';
import { font } from '../theme/palette';
import { Divider, EmptyState, ListRow } from '../ui/components';
import { Icon } from '../ui/icons';
import { usePalette } from '../ui/theme';

export default function WidgetsScreen() {
  const { account, roomId } = useLocalSearchParams<{ account: string; roomId: string }>();
  const store = useAppStore();
  const router = useRouter();
  const palette = usePalette();
  const listed = useStore((s) => accountOf(s, account)?.widgets[roomId]);

  useEffect(() => listWidgets(store, account, roomId), [store, account, roomId]);

  const open = (widget: Widget) =>
    Linking.openURL(widget.url).catch((error: Error) => Alert.alert(widget.name, String(error)));

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

  return (
    <>
      <Stack.Screen
        options={{
          title: t('widgets.title'),
          headerRight: listed?.can_add
            ? () => (
                <Pressable
                  testID="widget-add"
                  accessibilityRole="button"
                  accessibilityLabel={t('widgets.add')}
                  hitSlop={8}
                  onPress={() => router.push(routes.addWidget(account, roomId))}
                >
                  <Icon name="plus" size={22} color={palette.accent} />
                </Pressable>
              )
            : undefined,
        }}
      />
      <FlatList<Widget>
        testID="widgets"
        contentInsetAdjustmentBehavior="automatic"
        data={listed?.widgets ?? []}
        keyExtractor={(widget) => widget.id}
        ItemSeparatorComponent={() => <Divider inset={52} />}
        ListHeaderComponent={
          <Text style={[font.meta, styles.hint, { color: palette.textMuted }]}>
            {t('widgets.hint')}
          </Text>
        }
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
            icon="widgets"
            title={item.name}
            subtitle={item.host}
            onPress={() => {
              if (!item.can_remove) {
                void open(item);

                return;
              }

              Alert.alert(item.name, item.host, [
                { text: t('widgets.open'), onPress: () => void open(item) },
                {
                  text: t('widgets.remove'),
                  style: 'destructive',
                  onPress: () => askToRemove(item),
                },
                { text: t('common.cancel'), style: 'cancel' },
              ]);
            }}
          />
        )}
      />
    </>
  );
}

const styles = StyleSheet.create({
  hint: { paddingHorizontal: 16, paddingVertical: 12 },
});
