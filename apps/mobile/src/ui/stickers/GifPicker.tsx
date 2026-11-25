// The GIF tab of the sticker picker: GIPHY's trending GIFs, or those matching what is typed
// (searched once typing pauses, or on Search: a beta key allows 100 searches an hour), in rows in
// GIPHY's order, "Load more" for the next page of the search shown, and "Powered by GIPHY" as
// GIPHY asks. Previews play straight from GIPHY's URLs, exactly as given, cached in memory only
// (GIPHY forbids keeping its media). Tapping one sends its link and closes.
// Without a key (or with one GIPHY refuses) it asks for the user's own.

import { Image } from 'expo-image';
import { useEffect, useRef, useState } from 'react';
import {
  ActivityIndicator,
  Pressable,
  ScrollView,
  StyleSheet,
  Text,
  TextInput,
  useWindowDimensions,
  View,
} from 'react-native';
import type { FoundGif } from '@better-matrix/matrix-native/types';

import { locale, t } from '../../i18n';
import { searchGifs } from '../../state/actions/stickers';
import { useAppStore, useStore } from '../../state/react';
import {
  gifPageShown,
  gifSearchFailed,
  gifRows,
  gifTrouble,
  NO_GIFS,
  type GifResults,
  type GifTrouble,
} from '../../state/stickers';
import { font } from '../../theme/palette';
import { Button, EmptyState } from '../components';
import { Icon } from '../icons';
import { usePalette } from '../theme';
import { GiphyKeyForm } from './GiphyKeyForm';

/** How long typing must pause before searching. */
export const GIF_SEARCH_PAUSE_MS = 500;

const GAP = 6;

/** "Powered by GIPHY", from GIPHY's attribution marks (641 × 136). */
const GIPHY_MARK = require('../../../assets/images/giphy-powered-by.png');

const PAD = 12;

export function GifPicker({ onPick }: { onPick: (gif: FoundGif) => void }) {
  const store = useAppStore();
  const palette = usePalette();
  const key = useStore((s) => s.giphyKey);
  const { width } = useWindowDimensions();
  const [query, setQuery] = useState('');
  const [asked, setAsked] = useState({ query: '', now: true });
  const [results, setResults] = useState<GifResults>(NO_GIFS);
  const { gifs, next } = results;
  const [loading, setLoading] = useState(false);
  const [trouble, setTrouble] = useState<{ kind: GifTrouble; message: string } | null>(null);
  // Only the latest search's answer shows; typing something new drops those on their way.
  const latest = useRef(0);

  const search = (text: string, offset: number) => {
    if (!key) return;
    const id = ++latest.current;
    setLoading(true);
    searchGifs(store, key, text, offset, locale()).then(
      (page) => {
        if (id !== latest.current) return;
        setResults((shown) => gifPageShown(shown, text, offset, page));
        setTrouble(null);
        setLoading(false);
      },
      (error: Error) => {
        if (id !== latest.current) return;
        setResults((shown) => gifSearchFailed(shown, text, offset));
        setTrouble({ kind: gifTrouble(error.message), message: error.message });
        setLoading(false);
      },
    );
  };

  useEffect(() => {
    if (!key) return;
    const timer = setTimeout(() => search(asked.query, 0), asked.now ? 0 : GIF_SEARCH_PAUSE_MS);

    return () => clearTimeout(timer);
    // `search` reads the latest key; a new key or a new query searches again.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key, asked]);

  if (!key || trouble?.kind === 'no_key' || trouble?.kind === 'refused')
    return (
      <ScrollView keyboardShouldPersistTaps="handled" testID="gif-picker-key">
        {trouble?.kind === 'refused' ? (
          <Text style={[font.body, styles.refused, { color: palette.danger }]}>
            {t('mobile.stickers.gif_refused')}
          </Text>
        ) : null}
        <GiphyKeyForm replacing={trouble?.kind === 'refused'} onSaved={() => setTrouble(null)} />
      </ScrollView>
    );

  const count = Math.max(2, Math.floor((width - 2 * PAD) / 160));
  const tileWidth = (width - 2 * PAD - (count - 1) * GAP) / count;
  const rows = gifRows(gifs, count, tileWidth);

  return (
    <View style={styles.picker} testID="gif-picker">
      <View style={[styles.search, { backgroundColor: palette.surface }]}>
        <Icon name="search" size={18} color={palette.textMuted} />
        <TextInput
          testID="gif-search"
          value={query}
          onChangeText={(text) => {
            setQuery(text);
            latest.current++;
            setLoading(false);
            setAsked({ query: text, now: false });
          }}
          onSubmitEditing={() => setAsked({ query, now: true })}
          placeholder={t('mobile.stickers.gif_search')}
          placeholderTextColor={palette.textMuted}
          selectionColor={palette.accent}
          returnKeyType="search"
          autoCorrect={false}
          style={[font.body, styles.input, { color: palette.text }]}
        />
      </View>
      <ScrollView keyboardShouldPersistTaps="handled" contentContainerStyle={styles.content}>
        {trouble ? (
          <EmptyState
            icon="alert"
            title={t('mobile.stickers.gif_failed', { error: trouble.message })}
          />
        ) : gifs.length === 0 && !loading ? (
          <EmptyState icon="image" title={t('mobile.stickers.gif_none')} />
        ) : null}
        <View style={styles.rows}>
          {rows.map((row, index) => (
            <View key={index} style={styles.row}>
              {row.map(({ gif, height }) => (
                <Pressable
                  key={gif.id}
                  testID={`gif-${gif.id}`}
                  accessibilityRole="button"
                  accessibilityLabel={gif.title}
                  onPress={() => onPick(gif)}
                  style={({ pressed }) => [
                    styles.gif,
                    {
                      width: tileWidth,
                      height,
                      backgroundColor: palette.surface,
                      opacity: pressed ? 0.7 : 1,
                    },
                  ]}
                >
                  <Image
                    source={{ uri: gif.preview }}
                    style={StyleSheet.absoluteFill}
                    contentFit="contain"
                    cachePolicy="none"
                    autoplay
                    recyclingKey={gif.id}
                  />
                </Pressable>
              ))}
            </View>
          ))}
        </View>
        {loading ? <ActivityIndicator color={palette.textMuted} style={styles.more} /> : null}
        {!loading && next !== null && gifs.length > 0 ? (
          <View style={styles.more}>
            <Button
              testID="gif-more"
              kind="secondary"
              small
              title={t('mobile.stickers.gif_more')}
              onPress={() => search(results.query, next)}
            />
          </View>
        ) : null}
      </ScrollView>
      {/* GIPHY's own mark, always in view while GIFs are browsed, as GIPHY asks. */}
      <Image
        testID="gif-powered"
        source={GIPHY_MARK}
        style={styles.powered}
        contentFit="contain"
        accessibilityLabel={t('mobile.stickers.gif_powered')}
      />
    </View>
  );
}

const styles = StyleSheet.create({
  picker: { flex: 1 },
  search: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 8,
    marginHorizontal: PAD,
    marginBottom: 8,
    paddingHorizontal: 10,
    borderRadius: 10,
    minHeight: 38,
  },
  input: { flex: 1, paddingVertical: 8 },
  content: { paddingHorizontal: PAD, paddingBottom: 24 },
  rows: { gap: GAP },
  row: { flexDirection: 'row', gap: GAP },
  gif: { borderRadius: 8, overflow: 'hidden' },
  more: { alignItems: 'center', paddingTop: 12 },
  powered: { alignSelf: 'center', width: 85, height: 18, marginVertical: 8 },
  refused: { paddingHorizontal: 16, paddingTop: 12, textAlign: 'center' },
});
