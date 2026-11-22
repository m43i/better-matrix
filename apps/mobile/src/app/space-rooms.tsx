// A space's rooms (pushed from its sheet or settings): its rooms and subspaces in the space's
// order, each moved up or down or removed, and the user's other group rooms to add (ADR 0008,
// Space rooms). A move shows at once; the engine's room list confirms it.

import { Stack, useLocalSearchParams } from 'expo-router';
import { Alert, ScrollView, StyleSheet, View } from 'react-native';

import { t } from '../i18n';
import { changeSpace, orderSpace } from '../state/actions/admin';
import { reorder, roomsToAdd, spaceChildren, type SpaceChild } from '../state/admin';
import { shallow, useAppStore, useStore } from '../state/react';
import { account as accountOf, space as spaceOf } from '../state/selectors';
import { Avatar } from '../ui/Avatar';
import { Divider, IconButton, ListRow, ListSection } from '../ui/components';
import { usePalette } from '../ui/theme';

export default function SpaceRoomsScreen() {
  const { account, space: spaceId } = useLocalSearchParams<{ account: string; space: string }>();
  const store = useAppStore();
  const palette = usePalette();
  const space = useStore((s) => spaceOf(s, account, spaceId));
  const rooms = useStore((s) => accountOf(s, account)?.rooms ?? [], shallow);
  const spaces = useStore((s) => accountOf(s, account)?.spaces ?? [], shallow);

  // Only while the user may arrange the space: a right lost elsewhere takes the controls away.
  if (!space?.can_add_rooms) return null;
  const children = spaceChildren(space, rooms, spaces);
  const addable = roomsToAdd(space, rooms);

  const move = (roomId: string, delta: number) => {
    const order = reorder(space.children, roomId, delta);

    if (order) orderSpace(store, account, space.room_id, order).catch(() => {});
  };

  const remove = (child: SpaceChild) =>
    Alert.alert(
      t('mobile.admin.remove_title', { room: child.name, space: space.name }),
      t('mobile.admin.remove_body'),
      [
        { text: t('common.cancel'), style: 'cancel' },
        {
          text: t('common.remove'),
          style: 'destructive',
          onPress: () =>
            changeSpace(store, account, space.room_id, {
              type: 'Remove',
              data: child.roomId,
            }).catch(() => {}),
        },
      ],
    );

  return (
    <>
      <Stack.Screen options={{ title: t('mobile.admin.manage_rooms') }} />
      <ScrollView
        testID="space-rooms"
        style={{ backgroundColor: palette.bg }}
        contentContainerStyle={styles.content}
        contentInsetAdjustmentBehavior="automatic"
      >
        <ListSection
          title={t('mobile.admin.space_rooms')}
          footer={children.length > 1 ? t('mobile.admin.space_rooms_hint') : undefined}
        >
          {children.length === 0 ? <ListRow title={t('mobile.admin.no_space_rooms')} /> : null}
          {children.map((child, index) => (
            <View key={child.roomId}>
              {index > 0 ? <Divider inset={58} /> : null}
              <View style={styles.childRow}>
                <View style={styles.grow}>
                  <ListRow
                    testID={`space-room-${child.roomId}`}
                    title={child.name}
                    subtitle={
                      child.space
                        ? t('mobile.admin.subspace')
                        : child.joined
                          ? undefined
                          : t('mobile.admin.not_joined')
                    }
                    leading={
                      <Avatar
                        account={account}
                        id={child.roomId}
                        name={child.name}
                        media={
                          rooms.find((r) => r.room_id === child.roomId)?.avatar ??
                          spaces.find((s) => s.room_id === child.roomId)?.avatar ??
                          null
                        }
                        size={28}
                      />
                    }
                  />
                </View>
                <View style={styles.buttons}>
                  {index > 0 ? (
                    <IconButton
                      testID={`space-room-up-${child.roomId}`}
                      icon="arrowUp"
                      label={t('mobile.admin.move_up')}
                      color={palette.textMuted}
                      size={36}
                      onPress={() => move(child.roomId, -1)}
                    />
                  ) : (
                    <View style={styles.spacer} />
                  )}
                  {index < children.length - 1 ? (
                    <IconButton
                      testID={`space-room-down-${child.roomId}`}
                      icon="arrowDown"
                      label={t('mobile.admin.move_down')}
                      color={palette.textMuted}
                      size={36}
                      onPress={() => move(child.roomId, 1)}
                    />
                  ) : (
                    <View style={styles.spacer} />
                  )}
                  <IconButton
                    testID={`space-room-remove-${child.roomId}`}
                    icon="minusCircle"
                    label={t('mobile.admin.remove_from_space')}
                    color={palette.danger}
                    size={36}
                    onPress={() => remove(child)}
                  />
                </View>
              </View>
            </View>
          ))}
        </ListSection>
        <ListSection title={t('mobile.admin.add_rooms')}>
          {addable.length === 0 ? <ListRow title={t('mobile.admin.no_rooms_to_add')} /> : null}
          {addable.map((room, index) => (
            <View key={room.room_id}>
              {index > 0 ? <Divider inset={58} /> : null}
              <ListRow
                testID={`space-add-${room.room_id}`}
                title={room.name}
                leading={
                  <Avatar
                    account={account}
                    id={room.room_id}
                    name={room.name}
                    media={room.avatar}
                    size={28}
                  />
                }
                value={t('mobile.admin.add')}
                chevron={false}
                onPress={() =>
                  changeSpace(store, account, space.room_id, {
                    type: 'Add',
                    data: room.room_id,
                  }).catch(() => {})
                }
              />
            </View>
          ))}
        </ListSection>
      </ScrollView>
    </>
  );
}

const styles = StyleSheet.create({
  content: { paddingBottom: 32 },
  childRow: { flexDirection: 'row', alignItems: 'center' },
  grow: { flex: 1, minWidth: 0 },
  buttons: { flexDirection: 'row', alignItems: 'center', paddingRight: 8 },
  spacer: { width: 36 },
});
