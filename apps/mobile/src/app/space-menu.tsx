// A space's sheet (a long press on it in the space picker): its settings and its rooms for those
// who may change them, and leaving it, which asks which of its rooms to leave too.

import { useLocalSearchParams, useRouter } from 'expo-router';
import { StyleSheet, Text, View } from 'react-native';

import { t } from '../i18n';
import { routes } from '../routes';
import { useStore } from '../state/react';
import { space as spaceOf } from '../state/selectors';
import { font } from '../theme/palette';
import { Avatar } from '../ui/Avatar';
import { Divider, ListRow, ListSection } from '../ui/components';
import { usePalette } from '../ui/theme';

export default function SpaceMenuScreen() {
  const { account, space: spaceId } = useLocalSearchParams<{ account: string; space: string }>();
  const router = useRouter();
  const palette = usePalette();
  const space = useStore((s) => spaceOf(s, account, spaceId));

  if (!space) return null;

  /** Closes the sheet, then opens `href` over what it was opened from. */
  const open = (href: ReturnType<typeof routes.spaceRooms>) => {
    router.back();
    router.push(href);
  };

  return (
    // A plain view: the sheet fits its content (`fitToContents`), which a scroll view hides.
    <View testID="space-menu" style={[styles.content, { backgroundColor: palette.bg }]}>
      <View style={styles.header}>
        <Avatar
          account={account}
          id={space.room_id}
          name={space.name}
          media={space.avatar}
          size={36}
        />
        <Text style={[font.header, styles.grow, { color: palette.text }]} numberOfLines={1}>
          {space.name}
        </Text>
      </View>
      {space.can_change_settings || space.can_add_rooms ? (
        <ListSection>
          {space.can_change_settings ? (
            <ListRow
              testID="space-menu-settings"
              icon="settings"
              title={t('mobile.admin.space_settings_title')}
              onPress={() => open(routes.roomSettings(account, space.room_id))}
            />
          ) : null}
          {space.can_change_settings && space.can_add_rooms ? <Divider inset={52} /> : null}
          {space.can_add_rooms ? (
            <ListRow
              testID="space-menu-rooms"
              icon="space"
              title={t('mobile.admin.manage_rooms')}
              onPress={() => open(routes.spaceRooms(account, space.room_id))}
            />
          ) : null}
        </ListSection>
      ) : null}
      <ListSection>
        <ListRow
          testID="space-menu-leave"
          icon="leave"
          title={t('space.leave')}
          danger
          chevron={false}
          onPress={() => open(routes.leaveSpace(account, space.room_id))}
        />
      </ListSection>
    </View>
  );
}

const styles = StyleSheet.create({
  content: { paddingBottom: 32 },
  header: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 12,
    paddingHorizontal: 20,
    paddingTop: 20,
  },
  grow: { flex: 1 },
});
