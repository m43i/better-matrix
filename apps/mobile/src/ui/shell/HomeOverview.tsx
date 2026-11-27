// The greeting or space header above the room list in the current scope.

import { router } from 'expo-router';
import { StyleSheet, Text, View } from 'react-native';

import { t, tp } from '../../i18n';
import { routes } from '../../routes';
import { roomsInScope } from '../../state/sections';
import type { RoomListData } from '../../state/selectors';
import { font } from '../../theme/palette';
import { Avatar } from '../Avatar';
import { Button } from '../components';
import { Icon } from '../icons';
import { usePalette } from '../theme';

export function HomeOverview({ account, caughtUp }: { account: RoomListData; caughtUp: boolean }) {
  const palette = usePalette();
  const { summary, spaces, scope } = account;
  const space = scope === 'home' ? undefined : spaces.find((item) => item.room_id === scope.space);
  const name = space?.name ?? summary.display_name ?? summary.id;
  const title = space ? space.name : t('overview.welcome', { name: name || summary.id });

  const detail = space
    ? `${tp('overview.rooms', roomsInScope(account, scope, 'Group').length)} · ${tp('overview.members', space.members.length)}`
    : summary.id;

  return (
    <View testID="home-overview" style={styles.overview}>
      <View
        style={[styles.hero, { backgroundColor: palette.bubbleOwn, borderColor: palette.divider }]}
      >
        <View style={styles.heading}>
          <Avatar
            account={summary.id}
            id={space?.room_id ?? summary.id}
            name={name}
            media={space ? space.avatar : summary.avatar}
            size={46}
          />
          <View style={styles.words}>
            <Text style={[styles.title, { color: palette.text }]}>{title}</Text>
            <Text style={[font.meta, { color: palette.textMuted }]} numberOfLines={1}>
              {detail}
            </Text>
          </View>
        </View>
        {scope === 'home' ? (
          <View style={styles.actions}>
            <Button
              testID="overview-new-chat"
              icon="chat"
              kind="secondary"
              small
              title={t('create.chat')}
              onPress={() => router.push(routes.newChat())}
            />
            <Button
              testID="overview-saved"
              icon="bookmark"
              kind="secondary"
              small
              title={t('saved.title')}
              onPress={() => router.push(routes.saved(summary.id))}
            />
          </View>
        ) : null}
      </View>
      {caughtUp ? (
        <View
          style={[
            styles.caughtUp,
            { backgroundColor: palette.surface, borderColor: palette.divider },
          ]}
        >
          <Icon name="check" size={18} color={palette.accent} />
          <Text style={[font.bodyStrong, styles.words, { color: palette.text }]}>
            {t('overview.caught_up')}
          </Text>
        </View>
      ) : null}
    </View>
  );
}

const styles = StyleSheet.create({
  overview: { gap: 10, paddingHorizontal: 16, paddingTop: 8, paddingBottom: 12 },
  hero: {
    gap: 14,
    padding: 14,
    borderRadius: 16,
    borderCurve: 'continuous',
    borderWidth: StyleSheet.hairlineWidth,
  },
  heading: { flexDirection: 'row', alignItems: 'center', gap: 12 },
  words: { flex: 1, minWidth: 0, gap: 4 },
  title: { ...font.title, fontSize: 22 },
  actions: { flexDirection: 'row', flexWrap: 'wrap', gap: 8 },
  caughtUp: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 9,
    paddingHorizontal: 12,
    paddingVertical: 10,
    borderWidth: StyleSheet.hairlineWidth,
    borderRadius: 10,
    borderCurve: 'continuous',
  },
});
