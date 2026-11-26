import * as Location from 'expo-location';
import { useLocalSearchParams, useNavigation, useRouter } from 'expo-router';
import { useEffect, useRef, useState } from 'react';
import { ActivityIndicator, FlatList, StyleSheet, Text, TextInput, View } from 'react-native';

import { locale, t } from '../i18n';
import { keyOf } from '../state/actions/composer';
import { searchPlaces, sendLocation } from '../state/actions/messages';
import { coordinatesText, geoUri } from '../state/geo';
import { liveLocations } from '../engine/locationTask';
import { shareKey } from '../engine/liveLocation';
import { useStore, useAppStore } from '../state/react';
import { room } from '../state/selectors';
import { font } from '../theme/palette';
import { Button, Divider, ListRow } from '../ui/components';
import { Icon } from '../ui/icons';
import { usePalette } from '../ui/theme';
import { MapView } from '../ui/timeline/LocationCard';

/** A place that may be sent: where, what to call it, its map. */
interface Chosen {
  geoUri: string;
  description: string | null;
  estimated?: boolean;
  /** How far off the phone's position may be, in metres. */
  accuracy: number | null;
}

/** Where the phone's position stands: not asked for until the user wants it. */
type Here =
  | { type: 'idle' }
  | { type: 'locating' }
  | { type: 'found'; place: Chosen }
  | { type: 'denied' }
  | { type: 'failed'; error: string };

type Search =
  | { type: 'idle' }
  | { type: 'searching' }
  | { type: 'found'; places: Chosen[] }
  | { type: 'failed'; error: string };

async function findHere(ask: boolean): Promise<Here> {
  const permission = await (ask
    ? Location.requestForegroundPermissionsAsync()
    : Location.getForegroundPermissionsAsync());

  if (!permission.granted) return { type: 'denied' };

  let timer: ReturnType<typeof setTimeout> | undefined;

  const position = await Promise.race([
    Location.getCurrentPositionAsync({
      accuracy: Location.Accuracy.High,
      mayShowUserSettingsDialog: ask,
    }),
    new Promise<null>((resolve) => {
      timer = setTimeout(() => resolve(null), 10000);
    }),
  ]).finally(() => clearTimeout(timer));

  if (!position) return { type: 'failed', error: t('location.no_fix') };

  const { latitude, longitude, accuracy } = position.coords;
  const uri = geoUri(latitude, longitude, accuracy);

  return {
    type: 'found',
    place: { geoUri: uri, description: null, accuracy: accuracy ?? null },
  };
}

export default function LocationScreen() {
  const { account, roomId, thread } = useLocalSearchParams<{
    account: string;
    roomId: string;
    thread?: string;
  }>();

  const store = useAppStore();
  const router = useRouter();
  const navigation = useNavigation();
  const palette = usePalette();
  const [here, setHere] = useState<Here>({ type: 'idle' });
  const [query, setQuery] = useState('');
  const [search, setSearch] = useState<Search>({ type: 'idle' });
  const [picked, setPicked] = useState<Chosen | null>(null);

  const canShareLive = useStore((s) => room(s, account, roomId)?.can_share_live_location);
  const request = useStore((s) => s.liveShares[shareKey(account, roomId)]?.request);
  const owner = useStore((s) => s.accounts.find((a) => a.summary.id === account)?.summary);
  const [estimated, setEstimated] = useState<Chosen | null>(null);
  const generation = useRef(0);
  useEffect(() => {
    let mounted = true;
    const initial = generation.current;
    void (async () => {
      const fix = await findHere(false).catch(() => null);

      if (!mounted || generation.current !== initial) return;

      if (fix?.type === 'found') {
        setHere(fix);

        return;
      }

      await store.run({ type: 'Locate', data: { request_access: false } }, (event) => {
        if (mounted && event.type === 'PositionFound')
          setEstimated({
            geoUri: event.data.geo_uri,
            description: t('location.estimated'),
            accuracy: null,
            estimated: true,
          });
      });
    })().catch(() => {});

    return () => {
      mounted = false;
    };
  }, [store]);

  const locateHere = () => {
    setPicked(null);

    if (here.type === 'found' || here.type === 'locating') return;
    const attempt = ++generation.current;
    setHere({ type: 'locating' });
    findHere(true).then(
      (found) => {
        if (generation.current === attempt) setHere(found);
      },
      (cause: unknown) => {
        if (generation.current === attempt) setHere({ type: 'failed', error: String(cause) });
      },
    );
  };

  const chosen = picked ?? (here.type === 'found' ? here.place : estimated);

  const submit = () => {
    const term = query.trim();

    if (!term) return;
    setSearch({ type: 'searching' });
    searchPlaces(store, term, locale()).then(
      (places) =>
        setSearch({
          type: 'found',
          places: places.map((place) => ({
            geoUri: place.geo_uri,
            description: place.name,
            accuracy: null,
          })),
        }),
      (cause: unknown) =>
        setSearch({
          type: 'failed',
          error: cause instanceof Error ? cause.message : String(cause),
        }),
    );
  };

  const send = () => {
    if (!chosen) return;
    sendLocation(store, account, keyOf(roomId, thread), chosen.geoUri, chosen.description);
    router.back();
  };

  const status =
    here.type === 'idle'
      ? t('mobile.composer.location_pick')
      : here.type === 'locating'
        ? t('mobile.composer.location_locating')
        : here.type === 'denied'
          ? t('mobile.composer.location_denied')
          : here.type === 'failed'
            ? t('mobile.composer.location_failed', { error: here.error })
            : null;

  const header = (
    <View style={styles.header}>
      <Text style={[font.header, { color: palette.text }]}>
        {t('mobile.composer.location_title')}
      </Text>
      <View style={[styles.preview, { borderColor: palette.divider }]} testID="location-preview">
        <MapView
          account={account}
          uri={chosen?.geoUri}
          zoom={chosen?.estimated ? 10 : 15}
          person={{
            id: account,
            name: owner?.display_name ?? account,
            avatar: owner?.avatar ?? null,
          }}
        />
        <View style={styles.previewWords}>
          <Text style={[font.bodyStrong, { color: palette.text }]} numberOfLines={2}>
            {chosen?.description ?? t('mobile.composer.location_current')}
          </Text>
          <Text style={[font.meta, { color: palette.textMuted }]} numberOfLines={2}>
            {chosen
              ? [
                  coordinatesText(chosen.geoUri),
                  chosen.accuracy
                    ? t('mobile.composer.location_accuracy', {
                        meters: Math.round(chosen.accuracy),
                      })
                    : null,
                ]
                  .filter(Boolean)
                  .join(' · ')
              : status}
          </Text>
        </View>
      </View>
      <Button
        title={t('mobile.composer.location_send')}
        icon="send"
        onPress={send}
        disabled={!chosen}
        testID="location-send"
      />
      {!thread &&
        (request != null ? (
          <Button
            title={t('location.stop')}
            onPress={() => liveLocations.stop(store, request)}
            testID="location-stop"
          />
        ) : !canShareLive ? (
          <Text style={[font.meta, { color: palette.textMuted }]}>
            {t('location.permission_needed')}
          </Text>
        ) : (
          <Button
            title={t('location.start_live')}
            disabled={here.type !== 'found' || !!picked}
            onPress={async () => {
              if (
                here.type === 'found' &&
                (await liveLocations.start(store, account, roomId, here.place.geoUri)) &&
                navigation.isFocused()
              )
                router.back();
            }}
            testID="location-live"
          />
        ))}
      {estimated && !picked && (here.type === 'denied' || here.type === 'failed') && (
        <Text style={[font.meta, { color: palette.textMuted }]}>{status}</Text>
      )}
      {here.type !== 'found' || picked ? (
        <ListRow
          icon="myLocation"
          title={t(
            here.type === 'found'
              ? 'mobile.composer.location_current'
              : 'mobile.composer.location_use_mine',
          )}
          chevron={false}
          onPress={locateHere}
          testID="location-here"
        />
      ) : null}
      <View
        style={[styles.search, { backgroundColor: palette.surface, borderColor: palette.divider }]}
      >
        <Icon name="search" size={18} color={palette.textMuted} />
        <TextInput
          testID="location-search"
          value={query}
          onChangeText={setQuery}
          onSubmitEditing={submit}
          placeholder={t('mobile.composer.location_search')}
          accessibilityLabel={t('mobile.composer.location_search')}
          accessibilityHint={t('mobile.composer.location_search_hint')}
          placeholderTextColor={palette.textMuted}
          selectionColor={palette.accent}
          returnKeyType="search"
          autoCorrect={false}
          style={[font.body, styles.searchInput, { color: palette.text }]}
        />
        {search.type === 'searching' ? <ActivityIndicator color={palette.textMuted} /> : null}
      </View>
      {search.type === 'failed' ? (
        <Text style={[font.meta, { color: palette.danger }]}>
          {t('mobile.composer.location_search_failed', { error: search.error })}
        </Text>
      ) : null}
      {search.type === 'found' && search.places.length === 0 ? (
        <Text style={[font.meta, { color: palette.textMuted }]}>
          {t('mobile.composer.location_no_results')}
        </Text>
      ) : null}
    </View>
  );

  return (
    <FlatList
      testID="location"
      style={{ backgroundColor: palette.bg }}
      keyboardShouldPersistTaps="handled"
      data={search.type === 'found' ? search.places : []}
      keyExtractor={(place, index) => `${index}-${place.geoUri}`}
      ListHeaderComponent={header}
      ItemSeparatorComponent={() => <Divider inset={52} />}
      contentContainerStyle={styles.content}
      renderItem={({ item, index }) => (
        <ListRow
          icon="location"
          title={item.description ?? item.geoUri}
          subtitle={coordinatesText(item.geoUri) ?? undefined}
          chevron={false}
          selected={picked?.geoUri === item.geoUri}
          onPress={() => setPicked(item)}
          testID={`place-${index}`}
        />
      )}
    />
  );
}

const styles = StyleSheet.create({
  content: { paddingBottom: 24 },
  header: { gap: 12, paddingHorizontal: 16, paddingTop: 20, paddingBottom: 8 },
  preview: { borderRadius: 12, borderWidth: StyleSheet.hairlineWidth, overflow: 'hidden' },
  previewWords: { padding: 10, gap: 2 },
  search: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 8,
    paddingHorizontal: 10,
    borderRadius: 10,
    borderWidth: StyleSheet.hairlineWidth,
  },
  searchInput: { flex: 1, paddingVertical: 8 },
});
