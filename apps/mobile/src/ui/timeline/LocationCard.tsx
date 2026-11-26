// A shared place opens by event ID so the sheet follows live timeline updates.
import type { Location, Message, Person } from '@better-matrix/matrix-native/types';
import { Camera, Map, Marker } from '@maplibre/maplibre-react-native';
import { router } from 'expo-router';
import { useState } from 'react';
import { Pressable, StyleSheet, Text, View } from 'react-native';

import { t } from '../../i18n';
import { routes } from '../../routes';
import { coordinates, coordinatesText } from '../../state/geo';
import { useStore } from '../../state/react';
import { font } from '../../theme/palette';
import { Avatar } from '../Avatar';
import { Icon } from '../icons';
import { useNow } from '../shell/useNow';
import { usePalette } from '../theme';
import { useTimelineContext } from './context';
import { useContentWidth } from './readersLayout';

import darkMap from '../../../../../packages/matrix-core/maps/dark.json';
import lightMap from '../../../../../packages/matrix-core/maps/light.json';

const stylesByTheme = { dark: JSON.stringify(darkMap), light: JSON.stringify(lightMap) };

export function locationTitle(location: Location | undefined, now: number) {
  const until = location?.live_until;

  if (until != null) return until > now ? t('location.live') : t('location.ended');

  return location?.description ?? t('location.shared');
}

export function senderPin(message: Message): Person {
  return { id: message.sender_id, name: message.sender_name, avatar: message.sender_avatar };
}

export function LocationCard({ message, location }: { message: Message; location: Location }) {
  const palette = usePalette();
  const { account, timelineKey, thread, openActions } = useTimelineContext();
  const width = Math.min(300, useContentWidth());
  const title = locationTitle(location, useNow());
  const shown = useStore((s) => s.settings.showMaps);

  return (
    <Pressable
      onPress={() =>
        router.push(
          routes.locationMap(account, timelineKey.room_id, message.event_id, thread ?? undefined),
        )
      }
      onLongPress={() => openActions(message)}
      accessibilityRole="button"
      accessibilityLabel={title}
      style={[
        styles.card,
        { width, borderColor: palette.divider, backgroundColor: palette.surface },
      ]}
      testID="location-card"
    >
      <MapView account={account} uri={location.geo_uri} person={senderPin(message)} />
      <Text style={[font.bodyStrong, styles.caption, { color: palette.text }]} numberOfLines={2}>
        {title}
      </Text>
      {!shown && (
        <Text style={[font.meta, styles.caption, { color: palette.textMuted }]}>
          {coordinatesText(location.geo_uri) ?? t('location.waiting')}
        </Text>
      )}
    </Pressable>
  );
}

export function MapView({
  account,
  uri,
  person,
  interactive = false,
  zoom = 15,
}: {
  account: string;
  uri?: string;
  person: Person;
  interactive?: boolean;
  zoom?: number;
}) {
  const p = usePalette();
  const shown = useStore((s) => s.settings.showMaps);
  const point = coordinates(uri ?? '');

  const pin = (
    <View style={[styles.pin, { borderColor: p.accent }]}>
      <Avatar account={account} id={person.id} name={person.name} media={person.avatar} size={36} />
    </View>
  );

  const [failedStyle, setFailedStyle] = useState('');
  const style = p.dark ? 'dark' : 'light';
  const center: [number, number] | null = point ? [point.longitude, point.latitude] : null;

  if (!shown) return null;

  return (
    <View
      pointerEvents={interactive ? 'auto' : 'none'}
      style={[styles.map, { backgroundColor: p.surface }]}
    >
      {center ? (
        <>
          <Map
            style={StyleSheet.absoluteFill}
            mapStyle={stylesByTheme[style]}
            androidView="texture"
            attribution={interactive}
            logo={false}
            compass={interactive}
            scaleBar={false}
            dragPan={interactive}
            touchZoom={interactive}
            onDidFailLoadingMap={() => setFailedStyle(style)}
            onDidFinishLoadingMap={() => setFailedStyle('')}
          >
            <Camera center={center} zoom={zoom} duration={0} />
            {interactive && <Marker lngLat={center}>{pin}</Marker>}
          </Map>
          {!interactive && pin}
          <Text style={[styles.credit, { color: p.textMuted, backgroundColor: p.surface }]}>
            {t('location.credit')}
          </Text>
          {failedStyle === style && (
            <Text style={[styles.error, { color: p.text, backgroundColor: p.surface }]}>
              {t('location.unavailable')}
            </Text>
          )}
        </>
      ) : (
        <Icon name="location" size={28} color={p.textMuted} />
      )}
    </View>
  );
}

const styles = StyleSheet.create({
  card: {
    marginTop: 4,
    borderRadius: 12,
    borderWidth: StyleSheet.hairlineWidth,
    overflow: 'hidden',
  },
  caption: { paddingHorizontal: 10, paddingVertical: 6 },
  map: {
    width: '100%',
    aspectRatio: 1.7,
    alignItems: 'center',
    justifyContent: 'center',
    overflow: 'hidden',
  },
  pin: { width: 42, height: 42, borderWidth: 3, borderRadius: 21, overflow: 'hidden' },
  credit: {
    position: 'absolute',
    left: 4,
    bottom: 4,
    fontSize: 9,
    paddingHorizontal: 4,
    borderRadius: 3,
  },
  error: { position: 'absolute', top: 4, fontSize: 12, paddingHorizontal: 4 },
});
