// The emoji picker sheet: with `eventId` it reacts to that message (in the room's timeline, or
// the thread's when `thread` is given) and closes; without, it puts the emoji into that
// timeline's draft at the caret.

import { useLocalSearchParams, useRouter } from 'expo-router';

import { insertIntoDraft, keyOf, noteEmoji, react } from '../state/actions/composer';
import { useAppStore } from '../state/react';
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

  const pick = (emoji: string) => {
    if (eventId) {
      react(store, account, key, eventId, emoji);
    } else {
      noteEmoji(store, emoji);
      insertIntoDraft(store, account, key, emoji);
    }

    router.back();
  };

  return <EmojiPicker onPick={pick} />;
}
