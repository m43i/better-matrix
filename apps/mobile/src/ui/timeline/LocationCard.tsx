// A shared place: its map (drawn by the engine from OpenStreetMap, with a pin), what the sender
// called it and its coordinates, with OpenStreetMap's credit on the map. With maps off
// (Preferences, "Maps from OpenStreetMap") a pin stands in for the map and nothing is fetched.
// Tapping it opens the place on openstreetmap.org; a long press opens the message's actions.

import type { Location, MediaSize, Message } from '@better-matrix/matrix-native/types';
import { Image } from 'expo-image';
import { Pressable, StyleSheet, Text, View } from 'react-native';

import { t } from '../../i18n';
import { coordinatesText, OSM_CREDIT, webLink } from '../../state/geo';
import { useAppStore, useStore } from '../../state/react';
import { font } from '../../theme/palette';
import { Icon } from '../icons';
import { useMedia } from '../media';
import { usePalette } from '../theme';
import { useTimelineContext } from './context';
import { openLink } from './open';
import { useContentWidth } from './readersLayout';

/** The size maps are drawn at, 5:3, crisp on a phone's screen. */
export const MAP_SIZE: MediaSize = { type: 'Thumbnail', data: { width: 600, height: 360 } };

const CARD_WIDTH = 300;

export function LocationCard({ message, location }: { message: Message; location: Location }) {
  const palette = usePalette();
  const store = useAppStore();
  const { account, openActions } = useTimelineContext();
  const width = Math.min(CARD_WIDTH, useContentWidth());
  const link = webLink(location.geo_uri);
  const where = coordinatesText(location.geo_uri) ?? location.geo_uri;

  return (
    <Pressable
      onPress={link ? () => openLink(store, link, account) : undefined}
      onLongPress={() => openActions(message)}
      delayLongPress={350}
      accessibilityRole="link"
      accessibilityLabel={location.description ?? t('mobile.conversation.location')}
      accessibilityHint={t('mobile.conversation.location_open')}
      style={[
        styles.card,
        { width, borderColor: palette.divider, backgroundColor: palette.surface },
      ]}
      testID="location-card"
    >
      <MapView account={account} map={location.map} />
      <View style={styles.words}>
        <View style={styles.title}>
          <Icon name="location" size={16} color={palette.accent} />
          <Text style={[font.bodyStrong, styles.grow, { color: palette.text }]} numberOfLines={2}>
            {location.description ?? t('mobile.conversation.location')}
          </Text>
        </View>
        <Text style={[font.meta, { color: palette.textMuted }]} numberOfLines={1}>
          {where}
        </Text>
      </View>
    </Pressable>
  );
}

/**
 * A map at 5:3 with OpenStreetMap's credit; a pin until it is drawn, and in its place while maps
 * are off (then it is never fetched).
 */
export function MapView({ account, map }: { account: string; map: Location['map'] }) {
  const palette = usePalette();
  const shown = useStore((s) => s.settings.showMaps);
  const uri = useMedia(account, shown ? map : null, MAP_SIZE);

  return (
    <View style={[styles.map, { backgroundColor: palette.hover }]}>
      {uri ? (
        <Image source={{ uri }} style={StyleSheet.absoluteFill} contentFit="cover" />
      ) : (
        <View style={[StyleSheet.absoluteFill, styles.center]}>
          <Icon name="location" size={28} color={palette.textMuted} />
        </View>
      )}
      {shown ? (
        <Text style={[styles.credit, { color: '#1f2328', backgroundColor: '#ffffffcc' }]}>
          {OSM_CREDIT}
        </Text>
      ) : null}
    </View>
  );
}

const styles = StyleSheet.create({
  card: {
    marginTop: 4,
    borderRadius: 10,
    borderWidth: StyleSheet.hairlineWidth,
    overflow: 'hidden',
  },
  map: { width: '100%', aspectRatio: 5 / 3 },
  center: { alignItems: 'center', justifyContent: 'center' },
  credit: {
    position: 'absolute',
    right: 0,
    bottom: 0,
    fontSize: 10,
    paddingHorizontal: 4,
    paddingVertical: 1,
    borderTopLeftRadius: 4,
  },
  words: { padding: 10, gap: 2 },
  title: { flexDirection: 'row', alignItems: 'center', gap: 6 },
  grow: { flex: 1 },
});
