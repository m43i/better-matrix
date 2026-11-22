// A message's actions, from a long press (Slack's sheet): quick reactions and the picker, reply,
// reply in thread, copy, edit and delete the user's own, end their poll, save or share a file,
// and view its source.

import * as Clipboard from 'expo-clipboard';
import { router, useLocalSearchParams } from 'expo-router';
import { useMemo } from 'react';
import { Linking, Alert, Pressable, ScrollView, StyleSheet, Text, View } from 'react-native';

import { locale, t } from '../i18n';
import { setPinned, setSaved, messageLink, translationUrl } from '../state/actions/message-actions';
import { deleteMessage, react } from '../state/actions/timeline';
import { startEdit, startReply } from '../state/composer';
import {
  attachmentRows,
  editDraftOf,
  findMessage,
  messageActions,
  messageText,
} from '../state/messages';
import { useAppStore, useStore } from '../state/react';
import { previewHidden, room, timeline } from '../state/selectors';
import { roomKey, threadKey } from '../state/timeline';
import { routes } from '../routes';
import { Divider, EmptyState, ListRow } from '../ui/components';
import { Icon } from '../ui/icons';
import { usePalette } from '../ui/theme';
import { confirmEndPoll } from '../ui/timeline/PollCard';
import { AttachmentActions } from '../ui/timeline/AttachmentActions';

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

  const key = useMemo(
    () => (thread ? threadKey(roomId, thread) : roomKey(roomId)),
    [roomId, thread],
  );

  const message = useStore((s) => findMessage(timeline(s, account, key).entries, eventId));
  const summary = useStore((s) => room(s, account, roomId));
  // Held-back media offers no file actions here (ADR 0011), and then no divider either.
  const held = useStore((s) => (message ? previewHidden(s, account, roomId, message) : false));

  if (!message) {
    return (
      <View style={[styles.sheet, { backgroundColor: palette.bg }]}>
        <EmptyState icon="info" title={t('mobile.conversation.message_missing')} />
      </View>
    );
  }

  const can = messageActions(message, account, thread !== undefined, summary?.can_pin);
  const text = messageText(message);
  const attachment = message.content.type === 'Attachment' ? message.content.data : null;
  const close = () => router.back();
  const pinned = summary?.pinned.includes(eventId) ?? false;
  const saved = summary?.saved.includes(eventId) ?? false;

  const translate = () =>
    Alert.alert(t('actions.translate'), t('message.translate_explanation'), [
      { text: t('common.cancel'), style: 'cancel' },
      {
        text: t('actions.translate'),
        onPress: () => {
          if (text === null) return;
          void Linking.openURL(translationUrl(text, locale()))
            .then(close)
            .catch((error: Error) => Alert.alert(t('actions.translate'), String(error)));
        },
      },
    ]);

  const reactWith = (emoji: string) => {
    react(store, account, key, message.event_id, emoji);
    close();
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
    <ScrollView
      style={{ backgroundColor: palette.bg }}
      contentContainerStyle={styles.sheet}
      testID="message-actions"
    >
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
                style={[
                  styles.emoji,
                  { backgroundColor: mine ? `${palette.accent}33` : palette.surface },
                ]}
              >
                <Text style={styles.emojiText}>{emoji}</Text>
              </Pressable>
            );
          })}
          <Pressable
            onPress={() =>
              router.replace(
                routes.emoji(
                  account,
                  roomId,
                  thread ? { eventId: message.event_id, thread } : { eventId: message.event_id },
                ),
              )
            }
            accessibilityRole="button"
            accessibilityLabel={t('mobile.conversation.more_reactions')}
            style={[styles.emoji, { backgroundColor: palette.surface }]}
            testID="message-actions-more-reactions"
          >
            <Icon name="addReaction" size={22} color={palette.textMuted} />
          </Pressable>
        </View>
      ) : null}
      <View
        style={[styles.list, { backgroundColor: palette.surface, borderColor: palette.divider }]}
      >
        {can.file && attachment && attachmentRows(attachment, held, false).length > 0 ? (
          <>
            <AttachmentActions
              account={account}
              roomId={roomId}
              message={message}
              attachment={attachment}
              onDone={close}
            />
            <Divider inset={52} />
          </>
        ) : null}
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
        {can.forward ? (
          <ListRow
            icon="forward"
            title={t('actions.forward')}
            chevron={false}
            testID="message-actions-forward"
            onPress={() => router.replace(routes.forward(account, roomId, eventId, thread))}
          />
        ) : null}
        {can.edit ? (
          <ListRow
            icon="pencil"
            title={t('actions.edit')}
            chevron={false}
            testID="message-actions-edit"
            onPress={() => {
              const { draft, pills } = editDraftOf(message);
              startEdit(store, account, key, message, draft, pills);
              close();
            }}
          />
        ) : null}
        <Divider inset={52} />
        {can.link ? (
          <ListRow
            icon="link"
            title={t('actions.copy_link')}
            chevron={false}
            testID="message-actions-copy-link"
            onPress={() => {
              void Clipboard.setStringAsync(messageLink(roomId, eventId, account));
              close();
            }}
          />
        ) : null}
        {can.pin ? (
          <ListRow
            icon="pin"
            title={t(pinned ? 'actions.unpin' : 'actions.pin')}
            chevron={false}
            testID="message-actions-pin"
            onPress={() => {
              setPinned(store, account, roomId, eventId, !pinned);
              close();
            }}
          />
        ) : null}
        {can.save ? (
          <ListRow
            icon={saved ? 'unbookmark' : 'bookmark'}
            title={t(saved ? 'actions.unsave' : 'actions.save')}
            chevron={false}
            testID="message-actions-save"
            onPress={() => {
              setSaved(store, account, roomId, eventId, !saved);
              close();
            }}
          />
        ) : null}
        {can.copy && text !== null ? (
          <ListRow
            icon="copy"
            title={t('actions.copy_text')}
            chevron={false}
            testID="message-actions-copy"
            onPress={() => {
              void Clipboard.setStringAsync(text);
              close();
            }}
          />
        ) : null}
        {can.translate ? (
          <ListRow
            icon="translate"
            title={t('actions.translate')}
            chevron={false}
            testID="message-actions-translate"
            onPress={translate}
          />
        ) : null}
        {can.link ? (
          <ListRow
            icon="code"
            title={t('actions.view_source')}
            chevron={false}
            testID="message-actions-source"
            onPress={() => router.replace(routes.eventSource(account, roomId, eventId))}
          />
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
        {can.report ? (
          <>
            <Divider inset={52} />
            <ListRow
              icon="flag"
              title={t('report.action')}
              danger
              chevron={false}
              testID="message-actions-report"
              onPress={() => {
                close();
                router.push(
                  routes.report(
                    account,
                    { type: 'Message', data: { room_id: roomId, event_id: message.event_id } },
                    message.sender_name,
                    message.sender_id,
                  ),
                );
              }}
            />
          </>
        ) : null}
        {can.delete ? (
          <>
            <Divider inset={52} />
            <ListRow
              icon="trash"
              title={t('actions.delete')}
              danger
              chevron={false}
              testID="message-actions-delete"
              onPress={askDelete}
            />
          </>
        ) : null}
      </View>
    </ScrollView>
  );
}

const styles = StyleSheet.create({
  sheet: { paddingTop: 20, paddingBottom: 32, paddingHorizontal: 16, gap: 16 },
  quick: { flexDirection: 'row', justifyContent: 'space-between' },
  emoji: {
    width: 44,
    height: 44,
    borderRadius: 22,
    alignItems: 'center',
    justifyContent: 'center',
  },
  emojiText: { fontSize: 24 },
  list: { borderRadius: 12, overflow: 'hidden', borderWidth: StyleSheet.hairlineWidth },
});
