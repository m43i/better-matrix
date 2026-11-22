// The emoji picker sheet: with `eventId` it reacts to that message (in the room's timeline, or
// the thread's when `thread` is given) and closes; without, it puts the emoji into that
// timeline's draft at the caret, the room's custom emoji (image packs) offered first as pills.

import { useLocalSearchParams, useRouter } from 'expo-router';
import { useMemo } from 'react';

import { insertIntoDraft, keyOf, noteEmoji, react } from '../state/actions/composer';
import { emotePill } from '../state/pills';
import { useAppStore, useStore } from '../state/react';
import { packImages, roomPacks } from '../state/suggestions';
import { EmojiPicker } from '../ui/emoji/EmojiPicker';

export default function EmojiScreen() {
  const { account, roomId, eventId, thread } = useLocalSearchParams<{
    account: string;
    roomId: string;
    eventId?: string;
    thread?: string;
  }>();

  const store = useAppStore();
  const router = useRouter();
  const key = keyOf(roomId, thread);
  const packs = useStore((s) => roomPacks(s, account, roomId));

  const images = useMemo(
    () => (eventId ? [] : packImages(packs, 'emoticon').map(({ image }) => image)),
    [packs, eventId],
  );

  const pick = (emoji: string) => {
    if (eventId) {
      react(store, account, key, eventId, emoji);
    } else {
      noteEmoji(store, emoji);
      insertIntoDraft(store, account, key, emoji);
    }

    router.back();
  };

  return (
    <EmojiPicker
      onPick={pick}
      custom={{
        account,
        images,
        onPick: (image) => {
          const pill = emotePill(image.url, image.shortcode);
          insertIntoDraft(store, account, key, `${pill.label} `, pill);
          router.back();
        },
      }}
    />
  );
}
