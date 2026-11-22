// Sharing a place (sheet, from the composer's +): the phone's position first, once allowed and
// found, or a place found by name. Searching goes to OpenStreetMap only when the user submits
// (its policy forbids searching as they type). The chosen place shows on its map with
// OpenStreetMap's credit, and Send shares it to the room or thread the composer writes to.

import type { MediaRef } from '@better-matrix/matrix-native/types';
import * as Location from 'expo-location';
import { useLocalSearchParams, useRouter } from 'expo-router';
import { useEffect, useState } from 'react';
import { ActivityIndicator, FlatList, StyleSheet, Text, TextInput, View } from 'react-native';

import { locale, t } from '../i18n';
import { keyOf } from '../state/actions/composer';
import { searchPlaces, sendLocation } from '../state/actions/messages';
import { coordinatesText, geoUri, mapOf } from '../state/geo';
import { useAppStore } from '../state/react';
import { font } from '../theme/palette';
import { Button, Divider, ListRow } from '../ui/components';
import { Icon } from '../ui/icons';
import { usePalette } from '../ui/theme';
import { MapView } from '../ui/timeline/LocationCard';

/** A place that may be sent: where, what to call it, its map. */
interface Chosen {
  geoUri: string;
  description: string | null;
  map: MediaRef | null;
  /** How far off the phone's position may be, in metres. */
  accuracy: number | null;
}

/** Where the phone's position stands. */
type Here =
  | { type: 'locating' }
  | { type: 'found'; place: Chosen }
  | { type: 'denied' }
  | { type: 'failed'; error: string };

type Search =
  | { type: 'idle' }
  | { type: 'searching' }
  | { type: 'found'; places: Chosen[] }
  | { type: 'failed'; error: string };

async function findHere(): Promise<Here> {
  const permission = await Location.requestForegroundPermissionsAsync();

  if (!permission.granted) return { type: 'denied' };

  const position = await Location.getCurrentPositionAsync({
    accuracy: Location.Accuracy.High,
  });

  const { latitude, longitude, accuracy } = position.coords;
  const uri = geoUri(latitude, longitude, accuracy);

  return {
    type: 'found',
    place: { geoUri: uri, description: null, map: mapOf(uri), accuracy: accuracy ?? null },
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
  const palette = usePalette();
  const [here, setHere] = useState<Here>({ type: 'locating' });
  const [query, setQuery] = useState('');
  const [search, setSearch] = useState<Search>({ type: 'idle' });
  const [picked, setPicked] = useState<Chosen | null>(null);

  useEffect(() => {
    let live = true;
    findHere().then(
      (found) => {
        if (live) setHere(found);
      },
      (cause: unknown) => {
        if (live)
          setHere({
            type: 'failed',
            error: cause instanceof Error ? cause.message : String(cause),
          });
      },
    );

    return () => {
      live = false;
    };
  }, []);

  const chosen = picked ?? (here.type === 'found' ? here.place : null);

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
            map: place.map,
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
    here.type === 'locating'
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
        <MapView account={account} map={chosen?.map ?? null} />
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
      {picked && here.type === 'found' ? (
        <ListRow
          icon="myLocation"
          title={t('mobile.composer.location_current')}
          chevron={false}
          onPress={() => setPicked(null)}
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
