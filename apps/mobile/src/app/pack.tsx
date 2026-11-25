// One image pack: its name, its pictures in a grid, and what may be done with it. The user's own
// pack and a room's the user may change take a new name, pictures picked from the library (as
// stickers), and a tap on a picture to change or remove it; a room's pack can be used in all the
// user's rooms, and one they may change, deleted. Others' packs are read-only.

import type { ImagePack } from '@better-matrix/matrix-native/types';
import { Stack, useLocalSearchParams, useRouter } from 'expo-router';
import { useEffect, useState } from 'react';
import {
  ActivityIndicator,
  Alert,
  Pressable,
  ScrollView,
  StyleSheet,
  Text,
  useWindowDimensions,
  View,
} from 'react-native';

import { t } from '../i18n';
import { routes } from '../routes';
import { listImagePacks } from '../state/actions/messages';
import { addStickerFiles, editPack } from '../state/actions/stickers';
import { useAppStore, useStore } from '../state/react';
import { listedPacks, packTitle } from '../state/stickers';
import { font } from '../theme/palette';
import { pickPictures } from '../ui/composer/pickers';
import { Button, EmptyState, ListRow, ListSection, TextField } from '../ui/components';
import { PackPicture } from '../ui/composer/Suggestions';
import { usePalette } from '../ui/theme';

const CELL = 84;

export default function PackScreen() {
  const { account, packId, roomId } = useLocalSearchParams<{
    account: string;
    packId: string;
    roomId?: string;
  }>();

  const store = useAppStore();
  const palette = usePalette();
  const pack = useStore((s) => listedPacks(s, account, roomId).find((p) => p.id === packId));
  useEffect(() => listImagePacks(store, account, roomId ?? null), [store, account, roomId]);

  return (
    <>
      <Stack.Screen options={{ title: pack ? packTitle(pack) : '' }} />
      {pack ? (
        <PackPage account={account} roomId={roomId} pack={pack} />
      ) : (
        <View style={[styles.missing, { backgroundColor: palette.bg }]}>
          <EmptyState icon="sticker" title={t('mobile.stickers.pack_missing')} />
        </View>
      )}
    </>
  );
}

function PackPage({
  account,
  roomId,
  pack,
}: {
  account: string;
  roomId: string | undefined;
  pack: ImagePack;
}) {
  const store = useAppStore();
  const router = useRouter();
  const palette = usePalette();
  const { width } = useWindowDimensions();
  const [name, setName] = useState(pack.name);
  const [adding, setAdding] = useState(false);
  const columns = Math.max(3, Math.floor((width - 32) / CELL));
  const room = pack.place.type === 'Room';

  const rename = () => {
    if (name.trim() === pack.name.trim()) return;
    void editPack(store, account, pack.place, { type: 'Rename', data: name });
  };

  const add = async () => {
    const picked = await pickPictures();

    if (picked.length === 0) return;
    setAdding(true);
    await addStickerFiles(
      store,
      account,
      pack.place,
      picked.map((p) => p.path),
    );
    setAdding(false);
  };

  const askDelete = () =>
    Alert.alert(t('mobile.stickers.delete_pack_title'), t('mobile.stickers.delete_pack_body'), [
      { text: t('common.cancel'), style: 'cancel' },
      {
        text: t('mobile.stickers.delete_pack'),
        style: 'destructive',
        onPress: () => {
          void editPack(store, account, pack.place, { type: 'Delete' }).then((done) => {
            if (done) router.back();
          });
        },
      },
    ]);

  const rows = [];

  for (let i = 0; i < pack.images.length; i += columns)
    rows.push(pack.images.slice(i, i + columns));

  return (
    <ScrollView
      testID="pack"
      style={{ backgroundColor: palette.bg }}
      contentContainerStyle={styles.page}
      contentInsetAdjustmentBehavior="automatic"
      keyboardShouldPersistTaps="handled"
    >
      {pack.editable ? (
        <View style={styles.padded}>
          <TextField
            testID="pack-name"
            label={t('mobile.stickers.pack_name')}
            value={name}
            onChangeText={setName}
            placeholder={packTitle(pack)}
            returnKeyType="done"
            onSubmitEditing={rename}
            onBlur={rename}
            maxLength={100}
          />
        </View>
      ) : room ? (
        <Text style={[font.meta, styles.padded, { color: palette.textMuted }]}>
          {t('mobile.stickers.read_only')}
        </Text>
      ) : null}
      {room ? (
        <ListSection footer={t('mobile.stickers.everywhere_hint')}>
          <ListRow
            testID="pack-everywhere"
            icon="globe"
            title={t('mobile.stickers.everywhere')}
            toggle={{
              value: pack.everywhere,
              onChange: (on) =>
                void editPack(store, account, pack.place, { type: 'UseEverywhere', data: on }),
            }}
          />
        </ListSection>
      ) : null}
      {pack.images.length === 0 ? (
        <EmptyState icon="sticker" title={t('mobile.stickers.empty_pack')} />
      ) : (
        <View style={styles.grid}>
          {rows.map((row, index) => (
            <View key={index} style={styles.row}>
              {row.map((image) => (
                <Pressable
                  key={image.shortcode}
                  testID={`pack-image-${image.shortcode}`}
                  accessibilityRole={pack.editable ? 'button' : 'image'}
                  accessibilityLabel={`:${image.shortcode}: ${image.body}`}
                  disabled={!pack.editable}
                  onPress={() =>
                    router.push(routes.packImage(account, pack.id, image.shortcode, roomId))
                  }
                  style={({ pressed }) => [
                    styles.cell,
                    { backgroundColor: pressed ? palette.hover : 'transparent' },
                  ]}
                >
                  <PackPicture account={account} media={image.media} size={CELL - 20} />
                  <Text
                    style={[font.meta, { color: palette.textMuted }]}
                    numberOfLines={1}
                  >{`:${image.shortcode}:`}</Text>
                </Pressable>
              ))}
            </View>
          ))}
        </View>
      )}
      {pack.editable ? (
        <View style={styles.actions}>
          {adding ? (
            <View style={styles.adding}>
              <ActivityIndicator color={palette.textMuted} />
              <Text style={[font.body, { color: palette.textMuted }]}>
                {t('mobile.stickers.adding')}
              </Text>
            </View>
          ) : (
            <Button
              testID="pack-add"
              icon="plus"
              title={t('mobile.stickers.add')}
              onPress={() => void add()}
            />
          )}
          {room ? (
            <Button
              testID="pack-delete"
              kind="danger"
              title={t('mobile.stickers.delete_pack')}
              onPress={askDelete}
            />
          ) : null}
        </View>
      ) : null}
    </ScrollView>
  );
}

const styles = StyleSheet.create({
  page: { paddingVertical: 16, paddingBottom: 40, gap: 8 },
  padded: { paddingHorizontal: 16 },
  missing: { flex: 1 },
  grid: { paddingHorizontal: 12 },
  row: { flexDirection: 'row' },
  cell: {
    width: CELL,
    height: CELL + 16,
    borderRadius: 12,
    alignItems: 'center',
    justifyContent: 'center',
    gap: 2,
    paddingHorizontal: 4,
  },
  actions: { paddingHorizontal: 16, paddingTop: 8, gap: 8 },
  adding: {
    flexDirection: 'row',
    justifyContent: 'center',
    alignItems: 'center',
    gap: 8,
    minHeight: 44,
  },
});
