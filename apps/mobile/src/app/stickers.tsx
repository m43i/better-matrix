// The sticker and GIF picker (sheet, from the composer's +), Stickers and GIFs a segment apart.
// Stickers: those sent lately, then each pack's usable here (the user's own first, then the
// room's, then those used everywhere), filtered by what is typed, and Manage for the packs.
// GIFs: GIPHY, with the user's own key (`GifPicker`). Tapping either sends it to the room or
// thread the composer writes to, answering what it answers, and closes.

import { useLocalSearchParams, useRouter } from 'expo-router';
import { useEffect, useMemo, useState } from 'react';
import {
  Pressable,
  SectionList,
  StyleSheet,
  Text,
  TextInput,
  useWindowDimensions,
  View,
} from 'react-native';

import { t } from '../i18n';
import { routes } from '../routes';
import { keyOf } from '../state/actions/composer';
import { listImagePacks, sendSticker } from '../state/actions/messages';
import { sendGif } from '../state/actions/stickers';
import { shallow, useAppStore, useStore } from '../state/react';
import { stickerSections } from '../state/stickers';
import { roomPacks } from '../state/suggestions';
import { font } from '../theme/palette';
import { Button, EmptyState, IconButton } from '../ui/components';
import { PackPicture } from '../ui/composer/Suggestions';
import { Icon } from '../ui/icons';
import { Segments } from '../ui/people/Segments';
import { GifPicker } from '../ui/stickers/GifPicker';
import { usePalette } from '../ui/theme';

const CELL = 88;

type Tab = 'stickers' | 'gifs';

export default function StickersScreen() {
  const params = useLocalSearchParams<{
    account: string;
    roomId: string;
    thread?: string;
    tab?: string;
  }>();

  const { account, roomId, thread } = params;
  const router = useRouter();
  const store = useAppStore();
  const palette = usePalette();
  const [tab, setTab] = useState<Tab>(params.tab === 'gifs' ? 'gifs' : 'stickers');

  return (
    <View style={[styles.screen, { backgroundColor: palette.bg }]} testID="stickers">
      <View style={styles.tabs}>
        <Segments<Tab>
          testID="stickers-tabs"
          options={[
            { value: 'stickers', label: t('mobile.stickers.tab_stickers') },
            { value: 'gifs', label: t('mobile.stickers.tab_gifs') },
          ]}
          selected={tab}
          onChange={setTab}
        />
      </View>
      {tab === 'stickers' ? (
        <StickerTab account={account} roomId={roomId} thread={thread} />
      ) : (
        <GifPicker
          onPick={(gif) => {
            sendGif(store, account, keyOf(roomId, thread), gif);
            router.back();
          }}
        />
      )}
    </View>
  );
}

function StickerTab({
  account,
  roomId,
  thread,
}: {
  account: string;
  roomId: string;
  thread?: string;
}) {
  const store = useAppStore();
  const router = useRouter();
  const palette = usePalette();
  const { width } = useWindowDimensions();
  const [query, setQuery] = useState('');
  const columns = Math.max(3, Math.floor((width - 16) / CELL));
  const packs = useStore((s) => roomPacks(s, account, roomId));
  const recent = useStore((s) => s.settings.recentStickers[account] ?? [], shallow);
  useEffect(() => listImagePacks(store, account, roomId), [store, account, roomId]);

  const sections = useMemo(
    () =>
      stickerSections(packs, recent, query).map((section) => {
        const rows = [];

        for (let i = 0; i < section.stickers.length; i += columns)
          rows.push(section.stickers.slice(i, i + columns));

        return { key: section.key, title: section.title, data: rows };
      }),
    [packs, recent, query, columns],
  );

  const manage = () => {
    router.back();
    router.push(routes.packs(account, roomId));
  };

  const hasStickers = packs.some((pack) => pack.images.some((image) => image.sticker));

  return (
    <View style={styles.screen}>
      <View style={styles.bar}>
        <View style={[styles.search, { backgroundColor: palette.surface }]}>
          <Icon name="search" size={18} color={palette.textMuted} />
          <TextInput
            testID="sticker-search"
            value={query}
            onChangeText={setQuery}
            placeholder={t('mobile.stickers.search')}
            placeholderTextColor={palette.textMuted}
            selectionColor={palette.accent}
            autoCorrect={false}
            style={[font.body, styles.input, { color: palette.text }]}
          />
        </View>
        <IconButton
          testID="stickers-manage"
          icon="settings"
          label={t('mobile.stickers.manage')}
          color={palette.textMuted}
          size={38}
          onPress={manage}
        />
      </View>
      {!hasStickers ? (
        <View style={styles.empty}>
          <EmptyState
            icon="sticker"
            title={t('mobile.stickers.none')}
            body={t('mobile.stickers.none_hint')}
          />
          <Button testID="stickers-add" title={t('mobile.stickers.add')} onPress={manage} />
        </View>
      ) : sections.length === 0 ? (
        <EmptyState icon="search" title={t('mobile.stickers.no_match')} />
      ) : (
        <SectionList
          testID="sticker-list"
          sections={sections}
          keyboardShouldPersistTaps="handled"
          keyExtractor={(row, index) => `${index}-${row[0]?.url ?? ''}`}
          stickySectionHeadersEnabled={false}
          renderSectionHeader={({ section }) => (
            <Text style={[font.section, styles.header, { color: palette.textMuted }]}>
              {section.title}
            </Text>
          )}
          renderItem={({ item, section }) => (
            <View style={styles.row}>
              {item.map((image) => (
                <Pressable
                  key={`${image.shortcode}-${image.url}`}
                  testID={`sticker-${section.key === 'recent' ? 'recent-' : ''}${image.shortcode}`}
                  accessibilityRole="button"
                  accessibilityLabel={image.body}
                  onPress={() => {
                    sendSticker(store, account, keyOf(roomId, thread), image);
                    router.back();
                  }}
                  android_ripple={{ color: palette.hover, borderless: true }}
                  style={({ pressed }) => [
                    styles.cell,
                    { backgroundColor: pressed ? palette.hover : 'transparent' },
                  ]}
                >
                  <PackPicture account={account} media={image.media} size={CELL - 16} />
                </Pressable>
              ))}
            </View>
          )}
        />
      )}
    </View>
  );
}

const styles = StyleSheet.create({
  screen: { flex: 1 },
  tabs: { paddingHorizontal: 16, paddingTop: 20, paddingBottom: 10 },
  bar: { flexDirection: 'row', alignItems: 'center', gap: 4, paddingLeft: 12, paddingRight: 6 },
  search: {
    flex: 1,
    flexDirection: 'row',
    alignItems: 'center',
    gap: 8,
    paddingHorizontal: 10,
    borderRadius: 10,
    minHeight: 38,
  },
  input: { flex: 1, paddingVertical: 8 },
  empty: { alignItems: 'center', gap: 8, paddingBottom: 24 },
  header: { paddingHorizontal: 16, paddingTop: 12, paddingBottom: 4 },
  row: { flexDirection: 'row', paddingHorizontal: 8 },
  cell: {
    width: CELL,
    height: CELL,
    borderRadius: 12,
    alignItems: 'center',
    justifyContent: 'center',
  },
});
