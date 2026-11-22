// A message's edit history (sheet, from its "edited" label): every version, newest first, with
// when it was sent; the first one sent is marked as the original. Versions are drawn as the
// timeline draws messages, formatting and pills included.

import type { EditVersion } from '@better-matrix/matrix-native/types';
import { useLocalSearchParams } from 'expo-router';
import { useEffect, useMemo, useState } from 'react';
import { FlatList, StyleSheet, Text, View } from 'react-native';

import { t } from '../i18n';
import { listEdits } from '../state/actions/messages';
import { htmlBlocks, plainBlocks } from '../state/html';
import { shallow, useAppStore, useStore } from '../state/react';
import { editsKey } from '../state/reducer';
import { account as accountOf, messageSender } from '../state/selectors';
import { roomKey } from '../state/timeline';
import { sentAt } from '../state/timelineText';
import { font } from '../theme/palette';
import { Divider, EmptyState } from '../ui/components';
import { useNow } from '../ui/shell/useNow';
import { useShown } from '../ui/useShown';
import { usePalette } from '../ui/theme';
import {
  SpoilerContext,
  TimelineContext,
  type TimelineContextValue,
  useSpoilers,
} from '../ui/timeline/context';
import { type MediaOwner, RichText } from '../ui/timeline/RichText';

const NONE: EditVersion[] = [];

export default function EditHistoryScreen() {
  const { account, roomId, eventId } = useLocalSearchParams<{
    account: string;
    roomId: string;
    eventId: string;
  }>();

  const store = useAppStore();
  const palette = usePalette();
  const now = useNow();
  const [error, setError] = useState<string | null>(null);

  const versions = useStore(
    (s) => accountOf(s, account)?.edits[editsKey(roomId, eventId)] ?? NONE,
    shallow,
  );

  useEffect(() => {
    listEdits(store, account, roomId, eventId).catch((cause: unknown) =>
      setError(cause instanceof Error ? cause.message : String(cause)),
    );
  }, [store, account, roomId, eventId]);

  // Pills and links in a version act for this account, in this room, while the sheet is on show.
  const shown = useShown();

  const context = useMemo<TimelineContextValue>(
    () => ({
      account,
      timelineKey: roomKey(roomId),
      thread: null,
      // No bubbles here: every version is the same sender's.
      unnamed: new Set<string>(),
      scrollTo: () => undefined,
      openActions: () => undefined,
      shown: () => shown.current,
    }),
    [account, roomId, shown],
  );

  const spoilers = useSpoilers();

  // Every version's inline pictures follow the message's media; a sender not in any timeline
  // counts as someone else, whose pictures the account's choice may hold back.
  const sender = useStore((s) => messageSender(s, account, eventId)) ?? '';

  const media = useMemo<MediaOwner>(
    () => ({ sender_id: sender, event_id: eventId }),
    [sender, eventId],
  );

  const newestFirst = useMemo(
    () => versions.map((version, index) => ({ version, original: index === 0 })).reverse(),
    [versions],
  );

  return (
    <TimelineContext.Provider value={context}>
      <SpoilerContext.Provider value={spoilers}>
        <FlatList
          testID="edit-history"
          data={newestFirst}
          keyExtractor={(item) => item.version.event_id}
          contentContainerStyle={styles.content}
          ItemSeparatorComponent={() => <Divider inset={16} />}
          ListHeaderComponent={
            <Text style={[font.header, styles.title, { color: palette.text }]}>
              {t('mobile.conversation.edit_history')}
            </Text>
          }
          ListEmptyComponent={
            error ? (
              <EmptyState
                icon="alert"
                title={t('mobile.conversation.edit_history_failed', { error })}
              />
            ) : (
              <EmptyState title="" loading />
            )
          }
          renderItem={({ item: { version, original } }) => (
            <View style={styles.version} testID={`edit-${version.event_id}`}>
              <Text style={[font.meta, { color: palette.textMuted }]}>
                {original
                  ? `${sentAt(version.timestamp_ms, now)} · ${t('mobile.conversation.edit_history_original')}`
                  : sentAt(version.timestamp_ms, now)}
              </Text>
              <RichText
                blocks={version.html ? htmlBlocks(version.html) : plainBlocks(version.body)}
                owner={version.event_id}
                media={media}
              />
            </View>
          )}
        />
      </SpoilerContext.Provider>
    </TimelineContext.Provider>
  );
}

const styles = StyleSheet.create({
  content: { paddingBottom: 24 },
  title: { paddingHorizontal: 16, paddingTop: 20, paddingBottom: 8 },
  version: { paddingHorizontal: 16, paddingVertical: 10, gap: 4 },
});
