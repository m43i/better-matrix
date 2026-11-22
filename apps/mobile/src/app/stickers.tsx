// The sticker picker (sheet, from the composer's +): the stickers of the packs usable here (the
// user's own first, then the room's, then those enabled everywhere; MSC2545), a grid per pack.
// Tapping one sends it to the room or thread the composer writes to, and closes.

import { useLocalSearchParams, useRouter } from 'expo-router';
import { useEffect, useMemo } from 'react';
import { Pressable, SectionList, StyleSheet, Text, useWindowDimensions, View } from 'react-native';

import { t } from '../i18n';
import { keyOf } from '../state/actions/composer';
import { listImagePacks, sendSticker } from '../state/actions/messages';
import { useAppStore, useStore } from '../state/react';
import { packImages, roomPacks } from '../state/suggestions';
import { font } from '../theme/palette';
import { EmptyState } from '../ui/components';
import { PackPicture } from '../ui/composer/Suggestions';
import { usePalette } from '../ui/theme';

const CELL = 88;

export default function StickersScreen() {
  const { account, roomId, thread } = useLocalSearchParams<{
    account: string;
    roomId: string;
    thread?: string;
  }>();

  const store = useAppStore();
  const router = useRouter();
  const palette = usePalette();
  const { width } = useWindowDimensions();
  const columns = Math.max(3, Math.floor((width - 16) / CELL));
  const packs = useStore((s) => roomPacks(s, account, roomId));
  useEffect(() => listImagePacks(store, account, roomId), [store, account, roomId]);

  const sections = useMemo(
    () =>
      packs.flatMap((pack) => {
        const images = packImages([pack], 'sticker').map(({ image }) => image);
        const rows = [];

        for (let i = 0; i < images.length; i += columns) rows.push(images.slice(i, i + columns));

        return rows.length > 0 ? [{ key: pack.id, title: pack.name, data: rows }] : [];
      }),
    [packs, columns],
  );

  return (
    <View style={[styles.screen, { backgroundColor: palette.bg }]}>
      <Text style={[font.header, styles.title, { color: palette.text }]}>
        {t('mobile.composer.stickers_title')}
      </Text>
      {sections.length === 0 ? (
        <EmptyState
          icon="sticker"
          title={t('mobile.composer.stickers_none')}
          body={t('mobile.composer.stickers_none_hint')}
        />
      ) : (
        <SectionList
          testID="stickers"
          sections={sections}
          keyExtractor={(row, index) => `${index}-${row[0]?.url ?? ''}`}
          stickySectionHeadersEnabled={false}
          renderSectionHeader={({ section }) => (
            <Text style={[font.section, styles.header, { color: palette.textMuted }]}>
              {section.title}
            </Text>
          )}
          renderItem={({ item }) => (
            <View style={styles.row}>
              {item.map((image) => (
                <Pressable
                  key={`${image.shortcode}-${image.url}`}
                  testID={`sticker-${image.shortcode}`}
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
  title: { paddingHorizontal: 16, paddingTop: 20, paddingBottom: 4 },
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
