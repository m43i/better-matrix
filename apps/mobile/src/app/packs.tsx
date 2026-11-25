// Stickers & emoji: the packs to manage. The user's own (theirs in every room), the room's when
// opened for one (with New pack for those allowed to add one), and other rooms' packs the user
// uses everywhere. Each opens its pack.

import type { ImagePack } from '@better-matrix/matrix-native/types';
import { Stack, useLocalSearchParams, useRouter } from 'expo-router';
import { Fragment, useEffect } from 'react';
import { ScrollView, StyleSheet, View } from 'react-native';

import { tp, t } from '../i18n';
import { routes } from '../routes';
import { listImagePacks } from '../state/actions/messages';
import { useAppStore, useStore } from '../state/react';
import { listedPacks, mayAddPack, packGroups, packTitle } from '../state/stickers';
import { Divider, EmptyState, ListRow, ListSection } from '../ui/components';
import { PackPicture } from '../ui/composer/Suggestions';
import { usePalette } from '../ui/theme';

export default function PacksScreen() {
  const { account, roomId } = useLocalSearchParams<{ account: string; roomId?: string }>();
  const store = useAppStore();
  const palette = usePalette();
  const room = roomId ?? null;
  const packs = useStore((s) => listedPacks(s, account, room));
  const mayAdd = useStore((s) => mayAddPack(s, account, room));
  useEffect(() => listImagePacks(store, account, room), [store, account, room]);
  const groups = packGroups(packs, room);

  return (
    <>
      <Stack.Screen options={{ title: t('mobile.stickers.manager_title') }} />
      <ScrollView
        testID="packs"
        style={{ backgroundColor: palette.bg }}
        contentContainerStyle={styles.page}
        contentInsetAdjustmentBehavior="automatic"
      >
        {packs.length === 0 ? (
          <EmptyState loading title={t('mobile.stickers.manager_title')} />
        ) : null}
        {groups.own ? (
          <ListSection footer={t('mobile.stickers.yours_hint')}>
            <PackRow account={account} roomId={roomId} pack={groups.own} />
          </ListSection>
        ) : null}
        {room !== null && (groups.room.length > 0 || mayAdd) ? (
          <ListSection
            title={t('mobile.stickers.room_packs')}
            footer={t('mobile.stickers.room_packs_hint')}
          >
            {groups.room.map((pack, index) => (
              <Fragment key={pack.id}>
                {index > 0 ? <Divider inset={60} /> : null}
                <PackRow account={account} roomId={roomId} pack={pack} />
              </Fragment>
            ))}
            {mayAdd ? (
              <>
                {groups.room.length > 0 ? <Divider inset={60} /> : null}
                <NewPackRow account={account} roomId={room} />
              </>
            ) : null}
          </ListSection>
        ) : null}
        {groups.elsewhere.length > 0 ? (
          <ListSection
            title={t('mobile.stickers.elsewhere')}
            footer={t('mobile.stickers.elsewhere_hint')}
          >
            {groups.elsewhere.map((pack, index) => (
              <Fragment key={pack.id}>
                {index > 0 ? <Divider inset={60} /> : null}
                <PackRow account={account} roomId={roomId} pack={pack} />
              </Fragment>
            ))}
          </ListSection>
        ) : null}
      </ScrollView>
    </>
  );
}

function NewPackRow({ account, roomId }: { account: string; roomId: string }) {
  const router = useRouter();

  return (
    <ListRow
      testID="packs-new"
      icon="plus"
      title={t('mobile.stickers.new_pack')}
      onPress={() => router.push(routes.newPack(account, roomId))}
    />
  );
}

function PackRow({
  account,
  roomId,
  pack,
}: {
  account: string;
  roomId: string | undefined;
  pack: ImagePack;
}) {
  const router = useRouter();
  const picture = pack.avatar ?? pack.images[0]?.media ?? null;

  return (
    <ListRow
      testID={`pack-${pack.id}`}
      title={packTitle(pack)}
      subtitle={tp('mobile.stickers.count', pack.images.length)}
      leading={
        <View style={styles.picture}>
          {picture ? <PackPicture account={account} media={picture} size={36} /> : null}
        </View>
      }
      onPress={() => router.push(routes.pack(account, pack.id, roomId))}
    />
  );
}

const styles = StyleSheet.create({
  page: { paddingVertical: 16, paddingBottom: 40 },
  picture: { width: 36, height: 36, alignItems: 'center', justifyContent: 'center' },
});
