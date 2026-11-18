// A person's, room's or account's picture, or their initials on their identity colour: a
// rounded square, as the desktop's `components::avatar`.

import type { AccountId, MediaRef } from '@better-matrix/matrix-native/types';
import { Image, type ImageTransition } from 'expo-image';
import { StyleSheet, Text, View } from 'react-native';

import { identityColor } from '../theme/palette';
import { initials } from './initials';
import { AVATAR_SIZE, useMedia } from './media';
import { usePalette } from './theme';

const TRANSITION: ImageTransition = { duration: 120, skipOnCacheHit: 'memory' };

/** An avatar's corner radius: everyone and everything is a rounded square, as on the desktop. */
export function avatarRadius(size: number): number {
  return size * 0.22;
}

export function Avatar({
  account,
  id,
  name,
  media,
  size = 36,
  presence,
}: {
  /** Whose server to fetch the picture from. */
  account: AccountId | null;
  /** The Matrix ID the colour comes from. */
  id: string;
  name: string;
  media: MediaRef | null | undefined;
  size?: number;
  presence?: 'Online' | 'Away' | 'Offline' | 'Unknown';
}) {
  const palette = usePalette();
  const uri = useMedia(account, media, AVATAR_SIZE);
  const radius = avatarRadius(size);

  return (
    <View style={{ width: size, height: size }}>
      {uri ? (
        <Image
          source={{ uri }}
          // A recycled row shows no one else's face while its own loads, and a face already
          // in memory shows at once rather than fading in again as rows scroll back.
          recyclingKey={uri}
          style={{ width: size, height: size, borderRadius: radius }}
          transition={TRANSITION}
        />
      ) : (
        <View
          style={[
            styles.fallback,
            { width: size, height: size, borderRadius: radius, backgroundColor: identityColor(id) },
          ]}
        >
          <Text
            style={{ color: '#fff', fontWeight: '700', fontSize: size * 0.4 }}
            allowFontScaling={false}
          >
            {initials(name)}
          </Text>
        </View>
      )}
      {presence === 'Online' || presence === 'Away' ? (
        <View
          style={[
            styles.presence,
            {
              width: size * 0.32,
              height: size * 0.32,
              borderRadius: size * 0.16,
              borderColor: palette.bg,
              backgroundColor: presence === 'Online' ? palette.success : palette.warning,
            },
          ]}
        />
      ) : null}
    </View>
  );
}

const styles = StyleSheet.create({
  fallback: { alignItems: 'center', justifyContent: 'center' },
  presence: { position: 'absolute', right: -1, bottom: -1, borderWidth: 2 },
});
