// A conversation's places on one map (from a location card): the map fills the screen under a
// header with Open in navigation app and an X, and shows everyone's recent place in this room or
// thread as an avatar pin (`mapPins`). A card floating at the bottom lists them; tapping a row or
// a pin centres that person at street level, and the button at the top right comes back to them.
// Pins follow the timeline, so a live share moves as its fixes arrive.

import type { CameraRef } from '@maplibre/maplibre-react-native';
import { Camera, Map, Marker } from '@maplibre/maplibre-react-native';
import { Stack, useLocalSearchParams, useRouter } from 'expo-router';
import { useMemo, useRef, useState } from 'react';
import {
  Alert,
  Linking,
  Platform,
  Pressable,
  ScrollView,
  StyleSheet,
  Text,
  View,
} from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';

import { shareKey } from '../engine/liveLocation';
import { liveLocations } from '../engine/locationTask';
import { t } from '../i18n';
import { keyOf } from '../state/actions/composer';
import {
  coordinatesText,
  isLive,
  type MapPin,
  mapPins,
  navigationLink,
  pinDetail,
} from '../state/geo';
import { findMessage } from '../state/messages';
import { useAppStore, useStore } from '../state/react';
import { timeline } from '../state/selectors';
import { font } from '../theme/palette';
import { Avatar } from '../ui/Avatar';
import { Button, EmptyState, IconButton } from '../ui/components';
import { usePersonName } from '../ui/displayNames';
import { useToolbarIcon } from '../ui/shell/toolbarIcon';
import { useNow } from '../ui/shell/useNow';
import { headerTint, usePalette } from '../ui/theme';
import { mapStyle } from '../ui/timeline/LocationCard';

/** Street level, where a focused person is shown (the engine's `MAP_ZOOM`). */
const STREET_ZOOM = 15;

/** Hands a place to the platform's maps or navigation app. */
async function openInMaps(uri: string) {
  const link = navigationLink(uri, Platform.OS);

  if (!link) return;

  try {
    await Linking.openURL(link);
  } catch (error) {
    // Older devices have no default-navigation handler.
    if (Platform.OS === 'ios') {
      const point = coordinatesText(uri)?.replace(' ', '');

      try {
        await Linking.openURL(`https://maps.apple.com/?ll=${point}&q=${point}`);

        return;
      } catch (fallbackError) {
        Alert.alert(t('location.navigate'), String(fallbackError));

        return;
      }
    }

    Alert.alert(t('location.navigate'), String(error));
  }
}

export default function LocationMapScreen() {
  const { account, roomId, eventId, thread } = useLocalSearchParams<{
    account: string;
    roomId: string;
    eventId: string;
    thread?: string;
  }>();

  const router = useRouter();
  const palette = usePalette();
  const insets = useSafeAreaInsets();
  const now = useNow();
  const shown = useStore((s) => s.settings.showMaps);
  const entries = useStore((s) => timeline(s, account, keyOf(roomId, thread)).entries);
  const opened = useMemo(() => findMessage(entries, eventId), [entries, eventId]);
  const pins = useMemo(() => mapPins(entries, eventId, now), [entries, eventId, now]);
  // Who is focused, by sender (the card's, until another is picked): their pin stays focused as
  // their place moves.
  const [picked, setPicked] = useState<string | null>(null);
  const focusedSender = picked ?? opened?.sender_id;
  const focused = pins.find((pin) => pin.message.sender_id === focusedSender) ?? pins[0];
  const camera = useRef<CameraRef>(null);
  const [cardHeight, setCardHeight] = useState(0);
  const [failedStyle, setFailedStyle] = useState('');
  const style = palette.dark ? 'dark' : 'light';
  const tint = headerTint(palette);
  const externalIcon = useToolbarIcon('external', tint);
  const closeIcon = useToolbarIcon('close', tint);

  // Where the map first looks, once there is a place; later moves are the camera's own.
  const [start, setStart] = useState<[number, number] | null>(null);

  if (!start && focused) setStart([focused.longitude, focused.latitude]);

  const centre = (pin: MapPin) =>
    camera.current?.easeTo({
      center: [pin.longitude, pin.latitude],
      zoom: STREET_ZOOM,
      duration: 500,
    });

  const focus = (pin: MapPin) => {
    setPicked(pin.message.sender_id);
    centre(pin);
  };

  const header = (
    <>
      <Stack.Screen options={{ title: t('location.viewer_title') }} />
      <Stack.Toolbar placement="right">
        <Stack.Toolbar.Button
          icon={externalIcon}
          hidden={!focused || !externalIcon}
          accessibilityLabel={t('location.navigate')}
          onPress={() => focused && void openInMaps(focused.location.geo_uri)}
        />
        <Stack.Toolbar.Button
          icon={closeIcon}
          hidden={!closeIcon}
          accessibilityLabel={t('common.close')}
          onPress={() => router.back()}
        />
      </Stack.Toolbar>
    </>
  );

  if (!opened)
    return (
      <>
        {header}
        <EmptyState icon="info" title={t('mobile.conversation.message_missing')} />
      </>
    );

  const card = (
    <PeopleCard
      account={account}
      roomId={roomId}
      pins={pins}
      focused={focused}
      now={now}
      coordinates={!shown}
      onPress={focus}
    />
  );

  if (!shown)
    return (
      <View style={[styles.screen, { backgroundColor: palette.bg }]} testID="location-map">
        {header}
        <View style={[styles.plainCard, { paddingBottom: insets.bottom + 12 }]}>{card}</View>
      </View>
    );

  return (
    <View style={[styles.screen, { backgroundColor: palette.surface }]} testID="location-map">
      {header}
      {start ? (
        <Map
          style={StyleSheet.absoluteFill}
          mapStyle={mapStyle(style)}
          androidView="texture"
          attribution={false}
          logo={false}
          compass
          compassPosition={{ top: 64, right: 16 }}
          scaleBar={false}
          touchPitch={false}
          contentInset={{ bottom: cardHeight }}
          onDidFailLoadingMap={() => setFailedStyle(style)}
          onDidFinishLoadingMap={() => setFailedStyle('')}
        >
          <Camera ref={camera} initialViewState={{ center: start, zoom: STREET_ZOOM }} />
          {pins.map((pin) => (
            <Marker
              key={pin.message.sender_id}
              id={pin.message.sender_id}
              lngLat={[pin.longitude, pin.latitude]}
              anchor="bottom"
              offset={[0, PIN_DOT / 2]}
              onPress={() => focus(pin)}
            >
              <PersonPin
                account={account}
                pin={pin}
                focused={pin === focused}
                live={isLive(pin.location, now)}
              />
            </Marker>
          ))}
        </Map>
      ) : (
        <EmptyState icon="location" title={t('location.waiting')} />
      )}
      <Text style={[styles.credit, { color: palette.textMuted, backgroundColor: palette.bg }]}>
        {t('location.credit')}
      </Text>
      {failedStyle === style && (
        <Text style={[styles.error, { color: palette.text, backgroundColor: palette.bg }]}>
          {t('location.unavailable')}
        </Text>
      )}
      {focused && (
        <View style={styles.recenter}>
          <IconButton
            icon="myLocation"
            label={t('location.recenter')}
            onPress={() => centre(focused)}
            color={palette.accent}
            background={palette.bg}
            size={44}
            testID="location-recenter"
          />
        </View>
      )}
      <View
        style={[styles.floating, { bottom: insets.bottom + 12 }]}
        onLayout={(event) => setCardHeight(event.nativeEvent.layout.height + insets.bottom + 12)}
      >
        {card}
      </View>
    </View>
  );
}

/** The card listing the pins; one pin shows as its row alone. */
function PeopleCard({
  account,
  roomId,
  pins,
  focused,
  now,
  coordinates,
  onPress,
}: {
  account: string;
  roomId: string;
  pins: MapPin[];
  focused: MapPin | undefined;
  now: number;
  /** Say each place's coordinates, when there is no map to show them. */
  coordinates: boolean;
  onPress: (pin: MapPin) => void;
}) {
  const palette = usePalette();

  if (pins.length === 0)
    return (
      <View style={[styles.card, { backgroundColor: palette.bg, borderColor: palette.divider }]}>
        <Text style={[font.body, styles.waiting, { color: palette.textMuted }]}>
          {t('location.waiting')}
        </Text>
      </View>
    );

  return (
    <View
      style={[styles.card, { backgroundColor: palette.bg, borderColor: palette.divider }]}
      testID="location-people"
    >
      {pins.length > 1 && (
        <Text style={[font.section, styles.cardTitle, { color: palette.textMuted }]}>
          {t('location.on_map')}
        </Text>
      )}
      <ScrollView style={styles.rows} bounces={false}>
        {pins.map((pin) => (
          <PersonRow
            key={pin.message.event_id}
            account={account}
            roomId={roomId}
            pin={pin}
            focused={pin === focused && pins.length > 1}
            now={now}
            coordinates={coordinates}
            onPress={() => onPress(pin)}
          />
        ))}
      </ScrollView>
    </View>
  );
}

function PersonRow({
  account,
  roomId,
  pin,
  focused,
  now,
  coordinates,
  onPress,
}: {
  account: string;
  roomId: string;
  pin: MapPin;
  focused: boolean;
  now: number;
  coordinates: boolean;
  onPress: () => void;
}) {
  const store = useAppStore();
  const palette = usePalette();
  const { message } = pin;
  const own = message.sender_id === account;
  const senderName = usePersonName(message.sender_name);
  const name = own ? t('call.you') : senderName;
  const share = useStore((s) => s.liveShares[shareKey(account, roomId)]);

  const stop =
    own && isLive(pin.location, now) && share?.eventId === message.event_id ? share : null;

  const detail = pinDetail(pin, now);

  return (
    <Pressable
      testID={`location-person-${message.sender_id}`}
      accessibilityRole="button"
      accessibilityLabel={name}
      accessibilityHint={detail}
      accessibilityState={{ selected: focused }}
      onPress={onPress}
      android_ripple={{ color: palette.hover }}
      style={({ pressed }) => [
        styles.row,
        {
          backgroundColor: focused
            ? `${palette.accent}1f`
            : pressed && Platform.OS === 'ios'
              ? palette.hover
              : 'transparent',
        },
      ]}
    >
      <Avatar
        account={account}
        id={message.sender_id}
        name={senderName}
        media={message.sender_avatar}
        size={36}
      />
      <View style={styles.rowText}>
        <Text style={[font.bodyStrong, { color: palette.text }]} numberOfLines={1}>
          {name}
        </Text>
        <Text style={[font.meta, { color: palette.textMuted }]} numberOfLines={2}>
          {coordinates ? `${detail}\n${coordinatesText(pin.location.geo_uri) ?? ''}` : detail}
        </Text>
      </View>
      {stop && (
        <Button
          title={t('location.stop')}
          kind="danger"
          small
          onPress={() => liveLocations.stop(store, stop.request)}
          testID="location-stop"
        />
      )}
    </Pressable>
  );
}

/** The dot at a pin's exact point. */
const PIN_DOT = 10;

/**
 * A person's pin: their picture in a round frame with a pointer down to a dot at the place. The
 * focused pin is larger and framed in the accent; a live share's dot is the accent's.
 */
function PersonPin({
  account,
  pin,
  focused,
  live,
}: {
  account: string;
  pin: MapPin;
  focused: boolean;
  live: boolean;
}) {
  const palette = usePalette();
  const name = usePersonName(pin.message.sender_name);
  const size = focused ? 44 : 36;
  const frame = focused ? palette.accent : '#ffffff';

  return (
    <View style={styles.pin} accessibilityLabel={name}>
      <View
        style={[
          styles.pinFrame,
          { width: size + 8, height: size + 8, borderRadius: size / 2 + 4, backgroundColor: frame },
        ]}
      >
        <View style={{ width: size, height: size, borderRadius: size / 2, overflow: 'hidden' }}>
          <Avatar
            account={account}
            id={pin.message.sender_id}
            name={name}
            media={pin.message.sender_avatar}
            size={size}
          />
        </View>
      </View>
      <View style={[styles.pointer, { borderTopColor: frame }]} />
      <View style={[styles.dot, { backgroundColor: live ? palette.accent : palette.textMuted }]} />
    </View>
  );
}

const styles = StyleSheet.create({
  screen: { flex: 1 },
  plainCard: { flex: 1, justifyContent: 'flex-end', paddingHorizontal: 12 },
  floating: { position: 'absolute', left: 12, right: 12 },
  card: {
    borderRadius: 16,
    borderWidth: StyleSheet.hairlineWidth,
    paddingVertical: 6,
    shadowColor: '#000',
    shadowOpacity: 0.2,
    shadowRadius: 16,
    shadowOffset: { width: 0, height: 4 },
    elevation: 8,
  },
  cardTitle: { paddingHorizontal: 16, paddingTop: 6, paddingBottom: 4 },
  rows: { maxHeight: 232 },
  waiting: { padding: 16 },
  row: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 12,
    marginHorizontal: 6,
    paddingHorizontal: 10,
    paddingVertical: 8,
    borderRadius: 10,
  },
  rowText: { flex: 1, gap: 1 },
  recenter: {
    position: 'absolute',
    top: 12,
    right: 12,
    borderRadius: 22,
    shadowColor: '#000',
    shadowOpacity: 0.2,
    shadowRadius: 8,
    shadowOffset: { width: 0, height: 2 },
    elevation: 4,
  },
  credit: {
    position: 'absolute',
    top: 12,
    left: 12,
    fontSize: 9,
    paddingHorizontal: 4,
    borderRadius: 3,
    overflow: 'hidden',
  },
  error: {
    position: 'absolute',
    top: 36,
    alignSelf: 'center',
    fontSize: 12,
    paddingHorizontal: 6,
    borderRadius: 4,
    overflow: 'hidden',
  },
  pin: { alignItems: 'center' },
  pinFrame: {
    alignItems: 'center',
    justifyContent: 'center',
    shadowColor: '#000',
    shadowOpacity: 0.3,
    shadowRadius: 4,
    shadowOffset: { width: 0, height: 2 },
    elevation: 4,
  },
  pointer: {
    width: 0,
    height: 0,
    marginTop: -1,
    borderLeftWidth: 7,
    borderRightWidth: 7,
    borderTopWidth: 9,
    borderLeftColor: 'transparent',
    borderRightColor: 'transparent',
  },
  dot: {
    width: PIN_DOT,
    height: PIN_DOT,
    marginTop: 2,
    borderRadius: PIN_DOT / 2,
    borderWidth: 2,
    borderColor: '#ffffff',
  },
});
