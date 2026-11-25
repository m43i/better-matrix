// One picture of a pack (sheet, from the pack): its short code, its description, whether it is
// a sticker, an emoji or both, whether it is the pack's picture, and removing it.

import type { ImagePack, PackImage } from '@better-matrix/matrix-native/types';
import { useLocalSearchParams, useRouter } from 'expo-router';
import { useState } from 'react';
import { Alert, ScrollView, StyleSheet, View } from 'react-native';

import { t } from '../i18n';
import { editPack } from '../state/actions/stickers';
import { useAppStore, useStore } from '../state/react';
import { listedPacks, validShortcode } from '../state/stickers';
import { Button, EmptyState, ListRow, ListSection, TextField } from '../ui/components';
import { PackPicture } from '../ui/composer/Suggestions';
import { usePalette } from '../ui/theme';

export default function PackImageScreen() {
  const { account, packId, shortcode, roomId } = useLocalSearchParams<{
    account: string;
    packId: string;
    shortcode: string;
    roomId?: string;
  }>();

  const palette = usePalette();
  const pack = useStore((s) => listedPacks(s, account, roomId).find((p) => p.id === packId));
  const image = pack?.images.find((i) => i.shortcode === shortcode);

  if (!pack || !image)
    return (
      <View style={[styles.sheet, { backgroundColor: palette.bg }]}>
        <EmptyState icon="sticker" title={t('mobile.stickers.image_missing')} />
      </View>
    );

  return <ImageForm account={account} pack={pack} image={image} />;
}

function ImageForm({
  account,
  pack,
  image,
}: {
  account: string;
  pack: ImagePack;
  image: PackImage;
}) {
  const store = useAppStore();
  const router = useRouter();
  const palette = usePalette();
  const [code, setCode] = useState(image.shortcode);
  const [body, setBody] = useState(image.body === image.shortcode ? '' : image.body);
  const [sticker, setSticker] = useState(image.sticker);
  const [emoji, setEmoji] = useState(image.emoticon);
  const [busy, setBusy] = useState(false);
  const isPicture = pack.avatar_url === image.url;
  const valid = validShortcode(code.trim()) && (sticker || emoji);

  const save = async () => {
    if (!valid) return;
    setBusy(true);

    const done = await editPack(store, account, pack.place, {
      type: 'ChangeImage',
      data: {
        shortcode: image.shortcode,
        image: { shortcode: code.trim(), body, emoticon: emoji, sticker },
      },
    });

    setBusy(false);

    if (done) router.back();
  };

  const askRemove = () =>
    Alert.alert(t('mobile.stickers.remove_title'), t('mobile.stickers.remove_body'), [
      { text: t('common.cancel'), style: 'cancel' },
      {
        text: t('mobile.stickers.remove'),
        style: 'destructive',
        onPress: () => {
          void editPack(store, account, pack.place, {
            type: 'RemoveImage',
            data: image.shortcode,
          });
          router.back();
        },
      },
    ]);

  return (
    <ScrollView
      testID="pack-image"
      style={{ backgroundColor: palette.bg }}
      contentContainerStyle={styles.sheet}
      keyboardShouldPersistTaps="handled"
    >
      <View style={styles.picture}>
        <PackPicture account={account} media={image.media} size={120} />
      </View>
      <View style={styles.fields}>
        <TextField
          testID="pack-image-shortcode"
          label={t('mobile.stickers.shortcode')}
          value={code}
          onChangeText={setCode}
          autoCapitalize="none"
          autoCorrect={false}
          maxLength={100}
          error={validShortcode(code.trim()) ? null : t('mobile.stickers.shortcode_invalid')}
          hint={t('mobile.stickers.shortcode_hint')}
        />
        <TextField
          testID="pack-image-body"
          label={t('mobile.stickers.description')}
          value={body}
          onChangeText={setBody}
          placeholder={code}
          hint={t('mobile.stickers.description_hint')}
          maxLength={300}
        />
      </View>
      <ListSection footer={t('mobile.stickers.use_hint')}>
        <ListRow
          testID="pack-image-sticker"
          icon="sticker"
          title={t('mobile.stickers.as_sticker')}
          toggle={{ value: sticker, onChange: setSticker }}
        />
        <ListRow
          testID="pack-image-emoji"
          icon="smile"
          title={t('mobile.stickers.as_emoji')}
          toggle={{ value: emoji, onChange: setEmoji }}
        />
      </ListSection>
      <ListSection>
        <ListRow
          testID="pack-image-picture"
          icon="image"
          title={t('mobile.stickers.pack_picture')}
          checked={isPicture}
          onPress={() =>
            void editPack(store, account, pack.place, {
              type: 'SetAvatar',
              data: isPicture ? null : image.url,
            })
          }
        />
      </ListSection>
      <View style={styles.fields}>
        <Button
          testID="pack-image-save"
          title={t('mobile.stickers.save')}
          busy={busy}
          disabled={!valid}
          onPress={() => void save()}
        />
        <Button
          testID="pack-image-remove"
          kind="plain"
          title={t('mobile.stickers.remove')}
          onPress={askRemove}
        />
      </View>
    </ScrollView>
  );
}

const styles = StyleSheet.create({
  sheet: { paddingTop: 24, paddingBottom: 40, gap: 12 },
  picture: { alignItems: 'center' },
  fields: { paddingHorizontal: 16, gap: 12 },
});
