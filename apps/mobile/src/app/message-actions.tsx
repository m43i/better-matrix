// A message's actions, from a long press (Slack's sheet): quick reactions and the picker, reply,
// reply in thread, copy, edit and delete the user's own, end their poll, save or share a file.

import * as Clipboard from 'expo-clipboard';
import { router, useLocalSearchParams } from 'expo-router';
import { useMemo, useState } from 'react';
import { Alert, Pressable, ScrollView, StyleSheet, Text, View } from 'react-native';

import { t } from '../i18n';
import { deleteMessage, react } from '../state/actions/timeline';
import { startEdit, startReply } from '../state/composer';
import { editText, findMessage, messageActions, messageText } from '../state/messages';
import { useAppStore, useStore } from '../state/react';
import { timeline } from '../state/selectors';
import { roomKey, threadKey } from '../state/timeline';
import { routes } from '../routes';
import { Divider, EmptyState, ListRow } from '../ui/components';
import { Icon } from '../ui/icons';
import { usePalette } from '../ui/theme';
import { confirmEndPoll } from '../ui/timeline/PollCard';
import { saveAttachment, shareAttachment } from '../ui/timeline/files';

/** The quick reactions, as the desktop's picker offers them first. */
const QUICK = ['👍', '❤️', '😂', '🎉', '😮', '🙏'];

export default function MessageActionsScreen() {
  const { account, roomId, eventId, thread } = useLocalSearchParams<{
    account: string;
    roomId: string;
    eventId: string;
    thread?: string;
  }>();
  const palette = usePalette();
  const store = useAppStore();
  const key = useMemo(() => (thread ? threadKey(roomId, thread) : roomKey(roomId)), [roomId, thread]);
  const message = useStore((s) => findMessage(timeline(s, account, key).entries, eventId));
  const [busy, setBusy] = useState<'save' | 'share' | null>(null);
  if (!message) {
    return (
      <View style={[styles.sheet, { backgroundColor: palette.bg }]}>
        <EmptyState icon="info" title={t('mobile.conversation.message_missing')} />
      </View>
    );
  }
  const can = messageActions(message, account, thread !== undefined);
  const text = messageText(message);
  const attachment = message.content.type === 'Attachment' ? message.content.data : null;
  const close = () => router.back();
  const reactWith = (emoji: string) => {
    react(store, account, key, message.event_id, emoji);
    close();
  };
  const file = (kind: 'save' | 'share') => {
    if (!attachment) return;
    setBusy(kind);
    const done = kind === 'save' ? saveAttachment(store, account, attachment) : shareAttachment(store, account, attachment);
    void done.finally(() => {
      setBusy(null);
      close();
    });
  };
  const askDelete = () =>
    Alert.alert(t('confirm.delete_message_title'), t('confirm.delete_message_body'), [
      { text: t('common.cancel'), style: 'cancel' },
      {
        text: t('actions.delete'),
        style: 'destructive',
        onPress: () => {
          deleteMessage(store, account, key, message.event_id);
          close();
        },
      },
    ]);
  return (
    <ScrollView style={{ backgroundColor: palette.bg }} contentContainerStyle={styles.sheet} testID="message-actions">
      {can.react ? (
        <View style={styles.quick}>
          {QUICK.map((emoji) => {
            const mine = message.reactions.some((r) => r.key === emoji && r.mine);
            return (
              <Pressable
                key={emoji}
                onPress={() => reactWith(emoji)}
                accessibilityRole="button"
                accessibilityLabel={emoji}
                accessibilityState={{ selected: mine }}
                style={[styles.emoji, { backgroundColor: mine ? `${palette.accent}33` : palette.surface }]}
              >
                <Text style={styles.emojiText}>{emoji}</Text>
              </Pressable>
            );
          })}
          <Pressable
            onPress={() => router.replace(routes.emoji(account, roomId, thread ? { eventId: message.event_id, thread } : { eventId: message.event_id }))}
            accessibilityRole="button"
            accessibilityLabel={t('mobile.conversation.more_reactions')}
            style={[styles.emoji, { backgroundColor: palette.surface }]}
            testID="message-actions-more-reactions"
          >
            <Icon name="addReaction" size={22} color={palette.textMuted} />
          </Pressable>
        </View>
      ) : null}
      <View style={[styles.list, { backgroundColor: palette.surface, borderColor: palette.divider }]}>
        {can.reply ? (
          <ListRow
            icon="reply"
            title={t('actions.reply')}
            chevron={false}
            testID="message-actions-reply"
            onPress={() => {
              startReply(store, account, key, message);
              close();
            }}
          />
        ) : null}
        {can.thread ? (
          <ListRow
            icon="thread"
            title={t('actions.thread')}
            chevron={false}
            testID="message-actions-thread"
            onPress={() => {
              close();
              router.push(routes.thread(account, roomId, message.event_id));
            }}
          />
        ) : null}
        {can.copy && text !== null ? (
          <ListRow
            icon="copy"
            title={t('mobile.conversation.copy')}
            chevron={false}
            testID="message-actions-copy"
            onPress={() => {
              void Clipboard.setStringAsync(text);
              close();
            }}
          />
        ) : null}
        {can.edit ? (
          <ListRow
            icon="pencil"
            title={t('actions.edit')}
            chevron={false}
            testID="message-actions-edit"
            onPress={() => {
              startEdit(store, account, key, message, editText(message));
              close();
            }}
          />
        ) : null}
        {can.file && attachment ? (
          <>
            <ListRow
              icon="share"
              title={t('mobile.conversation.share')}
              chevron={false}
              value={busy === 'share' ? '…' : undefined}
              testID="message-actions-share"
              onPress={() => file('share')}
            />
            <ListRow
              icon="save"
              title={t('preview.download')}
              chevron={false}
              value={busy === 'save' ? '…' : undefined}
              testID="message-actions-save"
              onPress={() => file('save')}
            />
          </>
        ) : null}
        {can.endPoll && message.content.type === 'Poll' ? (
          <ListRow
            icon="poll"
            title={t('poll.end')}
            chevron={false}
            testID="message-actions-end-poll"
            onPress={() => {
              const question = message.content.type === 'Poll' ? message.content.data.question : '';
              confirmEndPoll(store, account, key, message.event_id, question, close);
            }}
          />
        ) : null}
        {can.delete ? (
          <>
            <Divider inset={52} />
            <ListRow icon="trash" title={t('actions.delete')} danger chevron={false} testID="message-actions-delete" onPress={askDelete} />
          </>
        ) : null}
      </View>
    </ScrollView>
  );
}

const styles = StyleSheet.create({
  sheet: { paddingTop: 20, paddingBottom: 32, paddingHorizontal: 16, gap: 16 },
  quick: { flexDirection: 'row', justifyContent: 'space-between' },
  emoji: { width: 44, height: 44, borderRadius: 22, alignItems: 'center', justifyContent: 'center' },
  emojiText: { fontSize: 24 },
  list: { borderRadius: 12, overflow: 'hidden', borderWidth: StyleSheet.hairlineWidth },
});
