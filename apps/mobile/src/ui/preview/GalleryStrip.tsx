// The preview's strip of thumbnails: every item of the gallery along the bottom, the one on
// screen ringed in the accent and scrolled into view; tapping one shows it.

import type { AccountId, Message } from '@better-matrix/matrix-native/types';
import { Image } from 'expo-image';
import { useEffect, useRef, useState } from 'react';
import { Pressable, ScrollView, type ScrollViewInstance, StyleSheet, View } from 'react-native';

import { t } from '../../i18n';
import { thumbnailOf } from '../../state/gallery';
import { Icon, type IconName } from '../icons';
import { PREVIEW_SIZE, useMedia } from '../media';
import { usePalette } from '../theme';

const THUMB = 52;

const GAP = 8;

const PAD = 12;

const RING = 2;

/** The strip's height above the safe area. */
export const STRIP_HEIGHT = THUMB + 2 * 10;

export function GalleryStrip({
  account,
  items,
  current,
  bottom,
  onPick,
}: {
  account: AccountId;
  items: readonly Message[];
  /** The event ID on screen. */
  current: string;
  /** The safe area below it. */
  bottom: number;
  onPick: (eventId: string) => void;
}) {
  const list = useRef<ScrollViewInstance>(null);
  const [width, setWidth] = useState(0);
  const index = items.findIndex((m) => m.event_id === current);

  // Centre the current thumbnail when the strip is wider than the screen.
  useEffect(() => {
    if (width <= 0 || index < 0) return;
    const x = PAD + index * (THUMB + GAP) - (width - THUMB) / 2;
    list.current?.scrollTo({ x: Math.max(0, x), animated: true });
  }, [index, width]);

  return (
    <View style={[styles.strip, { paddingBottom: bottom + 10 }]}>
      <ScrollView
        ref={list}
        horizontal
        showsHorizontalScrollIndicator={false}
        onLayout={(event) => setWidth(event.nativeEvent.layout.width)}
        contentContainerStyle={styles.content}
        accessibilityLabel={t('mobile.conversation.preview_gallery')}
        testID="preview-strip"
      >
        {items.map((message, i) => (
          <Thumb
            key={message.event_id}
            account={account}
            message={message}
            label={t('mobile.conversation.preview_item', {
              name: message.content.type === 'Attachment' ? message.content.data.name : '',
              index: i + 1,
              count: items.length,
            })}
            current={message.event_id === current}
            onPress={() => onPick(message.event_id)}
          />
        ))}
      </ScrollView>
    </View>
  );
}

function Thumb({
  account,
  message,
  label,
  current,
  onPress,
}: {
  account: AccountId;
  message: Message;
  label: string;
  current: boolean;
  onPress: () => void;
}) {
  const palette = usePalette();
  const attachment = message.content.type === 'Attachment' ? message.content.data : null;
  const picture = attachment ? thumbnailOf(attachment) : null;
  const uri = useMedia(account, picture?.media ?? null, picture?.size ?? PREVIEW_SIZE);

  const icon: IconName =
    attachment?.kind === 'Video'
      ? 'video'
      : attachment?.kind === 'Audio'
        ? attachment.audio?.voice
          ? 'mic'
          : 'audio'
        : 'image';

  return (
    <Pressable
      onPress={onPress}
      accessibilityRole="button"
      accessibilityLabel={label}
      accessibilityState={{ selected: current }}
      style={({ pressed }) => [
        styles.thumb,
        {
          borderColor: current ? palette.accent : 'transparent',
          opacity: pressed ? 0.6 : current ? 1 : 0.7,
        },
      ]}
      testID={`preview-thumb-${message.event_id}`}
    >
      {uri ? (
        <Image
          source={{ uri }}
          style={StyleSheet.absoluteFill}
          contentFit="cover"
          recyclingKey={uri}
        />
      ) : (
        <View style={[StyleSheet.absoluteFill, styles.tile]}>
          <Icon name={icon} size={22} color="#ffffff" />
        </View>
      )}
      {uri && attachment?.kind === 'Video' ? (
        <View style={[styles.badge, { backgroundColor: palette.scrim }]}>
          <Icon name="play" size={12} color="#ffffff" />
        </View>
      ) : null}
    </Pressable>
  );
}

const styles = StyleSheet.create({
  strip: {
    position: 'absolute',
    left: 0,
    right: 0,
    bottom: 0,
    paddingTop: 10,
    backgroundColor: '#00000080',
  },
  content: { flexGrow: 1, justifyContent: 'center', paddingHorizontal: PAD, gap: GAP },
  thumb: {
    width: THUMB,
    height: THUMB,
    borderRadius: 8,
    borderWidth: RING,
    overflow: 'hidden',
    backgroundColor: '#ffffff1f',
  },
  tile: { alignItems: 'center', justifyContent: 'center' },
  badge: {
    position: 'absolute',
    right: 3,
    bottom: 3,
    width: 18,
    height: 18,
    borderRadius: 9,
    alignItems: 'center',
    justifyContent: 'center',
  },
});
