// The composer under a room's or thread's timeline: a reply or edit line and pending attachments
// above one minimal row, + then the text field then the mic (or send once there is something to
// send). The field grows with its text up to a few lines, as the desktop's does. + opens every
// other option in the keyboard's place (photos, camera, files, poll, formatting, emoji); the
// formatting toolbar, once picked, sits above the row until it is closed. The screen around it
// (`ui/timeline/Conversation`) keeps it above the keyboard and the home indicator. The draft
// lives in the store per timeline, so it survives leaving the room; typing notices follow its
// changes.

import type { AccountId, TimelineKey } from '@better-matrix/matrix-native/types';
import { useFocusEffect, useRouter } from 'expo-router';
import { useCallback, useEffect, useRef, useState } from 'react';
import {
  Alert,
  Keyboard,
  Pressable,
  StyleSheet,
  Text,
  TextInput,
  type TextInputInstance,
  type TextInputSelectionChangeEvent,
  View,
} from 'react-native';

import { t } from '../../i18n';
import { routes } from '../../routes';
import {
  addAttachments,
  cancelEdit,
  cancelReply,
  removeAttachment,
  sendComposer,
  sendTyping,
  sendVoiceMemo,
  setDraft,
  setSelection,
} from '../../state/actions/composer';
import { composer, type PendingAttachment } from '../../state/composer';
import {
  activeBlocks,
  applyFormat,
  applyLink,
  continueBlock,
  type Format,
  hasText,
  type Selection,
} from '../../state/format';
import { excerpt, formatDuration } from '../../state/outgoing';
import { addNotice } from '../../state/reducer';
import { useAppStore, useStore } from '../../state/react';
import { room } from '../../state/selectors';
import { draftChanged, typingEnded, typingFor } from '../../state/typingNotices';
import { font } from '../../theme/palette';
import { IconButton } from '../components';
import { Icon } from '../icons';
import { usePalette } from '../theme';
import { AttachTray, type TrayChoice } from './AttachTray';
import { Attachments } from './Attachments';
import { Banner } from './Banner';
import { FormatBar } from './FormatBar';
import { DENIED, pickFiles, pickFromLibrary, takeWithCamera } from './pickers';
import { useVoiceMemo } from './useVoiceMemo';

export interface ComposerProps {
  account: AccountId;
  timelineKey: TimelineKey;
  /** The room's name, for the placeholder ("Message #general"). */
  roomName: string;
}

/** What shows besides the row: nothing, the formatting toolbar, link entry, or the + tray. */
type Tool =
  | { type: 'none' }
  | { type: 'format' }
  | { type: 'link'; url: string; selection: Selection }
  | { type: 'attach' };

const NONE: Tool = { type: 'none' };

export function Composer({ account, timelineKey, roomName }: ComposerProps) {
  const store = useAppStore();
  const palette = usePalette();
  const router = useRouter();
  const state = useStore((s) => composer(s, account, timelineKey));
  const direct = useStore((s) => room(s, account, timelineKey.room_id)?.kind === 'Direct');
  const roomId = timelineKey.room_id;
  const thread = timelineKey.thread;

  const field = useRef<TextInputInstance>(null);
  const [text, setText] = useState(state.draft);
  /** The draft as this view last wrote or took it: anything else in the store came from elsewhere. */
  const written = useRef(state.draft);
  const selection = useRef<Selection>({ start: state.draft.length, end: state.draft.length });
  const [caret, setCaret] = useState(state.draft.length);
  /** Where the caret goes once the field shows a text this view changed. */
  const pendingSelection = useRef<Selection | null>(null);
  const [tool, setTool] = useState<Tool>(NONE);
  const typing = useRef(typingFor(state.draft));

  const notify = useCallback(
    (notice: boolean | null) => {
      if (notice !== null) sendTyping(store, account, roomId, notice);
    },
    [store, account, roomId],
  );

  // A draft changed elsewhere (an emoji picked, an edit started or dropped, a send) comes in.
  useEffect(() => {
    if (state.draft === written.current) return;
    written.current = state.draft;
    setText(state.draft);
    pendingSelection.current = state.selection ?? {
      start: state.draft.length,
      end: state.draft.length,
    };
    // Not the user typing: the next change they make is measured from here.
    typing.current = { ...typing.current, draft: state.draft };
  }, [state.draft, state.selection]);

  // The caret goes where a change put it once the field shows the change.
  useEffect(() => {
    const next = pendingSelection.current;

    if (!next) return;
    pendingSelection.current = null;
    selection.current = next;
    field.current?.setSelection(next.start, next.end);
  }, [text]);

  // Starting a reply or an edit (from the message's sheet) puts the caret in the field, once the
  // sheet has gone.
  const target = state.editing?.event_id ?? state.replyTo?.event_id ?? null;
  useEffect(() => {
    if (target === null) return;
    const timer = setTimeout(() => field.current?.focus(), 350);

    return () => clearTimeout(timer);
  }, [target]);

  // Leaving the conversation (another screen on top, or back) ends typing there.
  useFocusEffect(
    useCallback(
      () => () => {
        const step = typingEnded(typing.current);
        typing.current = step.next;
        notify(step.notice);
      },
      [notify],
    ),
  );

  /** Takes a new draft from the user or the toolbar. */
  const write = (draft: string, caretAt?: Selection) => {
    written.current = draft;
    setText(draft);

    if (caretAt) pendingSelection.current = caretAt;
    setDraft(store, account, timelineKey, draft);
    const step = draftChanged(typing.current, draft, Date.now());
    typing.current = step.next;
    notify(step.notice);
  };

  const onChangeText = (next: string) => {
    const continued = continueBlock(written.current, next);

    if (continued) write(continued.text, continued.selection);
    else write(next);
  };

  const onSelectionChange = (event: TextInputSelectionChangeEvent) => {
    selection.current = event.nativeEvent.selection;
    setCaret(event.nativeEvent.selection.start);
  };

  const format = (which: Format) => {
    if (which === 'link') {
      setTool({ type: 'link', url: 'https://', selection: selection.current });

      return;
    }

    const edited = applyFormat(written.current, selection.current, which);
    write(edited.text, edited.selection);
  };

  const confirmLink = () => {
    if (tool.type !== 'link') return;
    const edited = applyLink(written.current, tool.selection, tool.url);
    write(edited.text, edited.selection);
    setTool({ type: 'format' });
    field.current?.focus();
  };

  const send = () => {
    const step = typingEnded(typing.current, '');
    typing.current = step.next;
    notify(step.notice);
    sendComposer(store, account, timelineKey);
  };

  const notice = (message: string) => store.update((s) => addNotice(s, message, account));

  const voice = useVoiceMemo(
    (path, durationMs) => sendVoiceMemo(store, account, timelineKey, path, durationMs),
    (message) => notice(message),
  );

  const attach = async (pick: () => Promise<PendingAttachment[] | typeof DENIED>) => {
    try {
      const picked = await pick();

      if (picked === DENIED) {
        Alert.alert(t('mobile.composer.camera'), t('mobile.composer.camera_denied'));

        return;
      }

      addAttachments(store, account, timelineKey, picked);

      if (picked.length > 0) setTool(NONE);
    } catch (error) {
      notice(
        t('mobile.composer.attach_failed', {
          error: error instanceof Error ? error.message : String(error),
        }),
      );
    }
  };

  const openEmoji = () => {
    setSelection(store, account, timelineKey, selection.current);
    router.push(routes.emoji(account, roomId, thread ? { thread } : {}));
  };

  const choose = (choice: TrayChoice) => {
    switch (choice) {
      case 'library':
        void attach(pickFromLibrary);
        break;
      case 'camera':
        void attach(takeWithCamera);
        break;
      case 'files':
        void attach(pickFiles);
        break;
      case 'poll':
        setTool(NONE);
        router.push(routes.newPoll(account, roomId, thread ?? undefined));
        break;
      case 'format':
        setTool({ type: 'format' });
        field.current?.focus();
        break;
      case 'emoji':
        setTool(NONE);
        openEmoji();
        break;
    }
  };

  const toggleTray = () => {
    if (tool.type === 'attach') {
      setTool(NONE);
      field.current?.focus();
    } else {
      Keyboard.dismiss();
      setTool({ type: 'attach' });
    }
  };

  const attachments = state.attachments ?? [];
  const editing = state.editing !== null;
  const sendable = hasText(text) || (!editing && attachments.length > 0);
  const recording = voice.state.phase !== 'idle';

  const held =
    (voice.state.phase === 'recording' || voice.state.phase === 'starting') && voice.state.held;

  const name = roomName.startsWith('#') || direct ? roomName : `#${roomName}`;
  const placeholder = thread ? t('thread.placeholder') : t('composer.placeholder', { room: name });

  /**
   * The button at the right: send, or the mic. It stays one element while a memo records, so a
   * hold that started on the mic keeps its touch until it lets go.
   */
  const action = (() => {
    if (recording) {
      return held
        ? {
            icon: 'mic' as const,
            label: t('mobile.composer.release_to_send'),
            background: palette.danger,
          }
        : { icon: 'send' as const, label: t('composer.voice_send'), background: palette.accent };
    }

    if (sendable || editing) {
      return {
        icon: editing ? ('check' as const) : ('send' as const),
        label: editing ? t('common.save') : t('composer.send'),
        background: palette.accent,
      };
    }

    return { icon: 'mic' as const, label: t('composer.voice'), background: 'transparent' };
  })();

  const actionDisabled = !recording && editing && !sendable;

  const onAction = () => {
    if (recording) {
      if (!held) voice.dispatch({ type: 'send' });
    } else if (sendable || editing) {
      send();
    } else {
      voice.dispatch({ type: 'press', held: false });
    }
  };

  const onActionLongPress = () => {
    if (!recording && !sendable && !editing) voice.dispatch({ type: 'press', held: true });
  };

  return (
    <View
      style={[styles.composer, { backgroundColor: palette.bg, borderTopColor: palette.divider }]}
      testID="composer"
    >
      {state.replyTo && !editing ? (
        <Banner
          icon="reply"
          title={t('composer.reply_to', { name: state.replyTo.sender_name })}
          quote={excerpt(state.replyTo)}
          cancelLabel={t('composer.reply_cancel')}
          onCancel={() => cancelReply(store, account, timelineKey)}
          testID="composer-reply"
        />
      ) : null}
      {editing ? (
        <Banner
          icon="pencil"
          title={t('composer.editing')}
          cancelLabel={t('composer.edit_cancel')}
          onCancel={() => cancelEdit(store, account, timelineKey)}
          testID="composer-edit"
        />
      ) : null}
      {recording ? null : (
        <Attachments
          attachments={attachments}
          onRemove={(path) => removeAttachment(store, account, timelineKey, path)}
        />
      )}
      {tool.type === 'link' ? (
        <View style={[styles.link, { borderColor: palette.divider }]}>
          <Icon name="link" size={18} color={palette.textMuted} />
          <TextInput
            testID="composer-link"
            autoFocus
            value={tool.url}
            onChangeText={(url) => setTool({ ...tool, url })}
            onSubmitEditing={confirmLink}
            placeholder={t('mobile.composer.link_address')}
            accessibilityLabel={t('mobile.composer.link_address')}
            placeholderTextColor={palette.textMuted}
            selectionColor={palette.accent}
            keyboardType="url"
            autoCapitalize="none"
            autoCorrect={false}
            returnKeyType="done"
            style={[font.body, styles.linkInput, { color: palette.text }]}
          />
          <IconButton
            icon="check"
            label={t('composer.link_apply')}
            onPress={confirmLink}
            size={32}
            color={palette.accent}
            testID="composer-link-apply"
          />
          <IconButton
            icon="close"
            label={t('composer.link_cancel')}
            onPress={() => setTool({ type: 'format' })}
            size={32}
            color={palette.textMuted}
            testID="composer-link-cancel"
          />
        </View>
      ) : null}
      {(tool.type === 'format' || tool.type === 'link') && !recording ? (
        <View style={styles.formatBar}>
          <IconButton
            icon="textFormat"
            label={t('composer.formatting')}
            onPress={() => setTool(NONE)}
            size={36}
            color={palette.accent}
            background={palette.hover}
            testID="composer-format"
          />
          <View style={styles.formats}>
            <FormatBar active={activeBlocks(text, caret)} onFormat={format} />
          </View>
        </View>
      ) : null}
      <View style={styles.row}>
        {recording ? (
          <IconButton
            icon="trash"
            label={t('composer.voice_cancel')}
            onPress={() => voice.dispatch({ type: 'discard' })}
            size={36}
            color={palette.danger}
            testID="composer-voice-cancel"
          />
        ) : (
          <IconButton
            icon={tool.type === 'attach' ? 'close' : 'plus'}
            label={tool.type === 'attach' ? t('common.close') : t('composer.attach')}
            onPress={toggleTray}
            size={36}
            color={palette.text}
            background={palette.surface}
            testID="composer-attach"
          />
        )}
        <View style={[styles.field, { backgroundColor: palette.surface }]}>
          {recording ? (
            <View
              style={styles.recording}
              testID="composer-recording"
              accessibilityLiveRegion="polite"
            >
              <View style={[styles.dot, { backgroundColor: palette.danger }]} />
              <Text
                style={[font.bodyStrong, { color: palette.text, fontVariant: ['tabular-nums'] }]}
              >
                {formatDuration(voice.elapsed)}
              </Text>
              <Text style={[font.meta, { color: palette.textMuted, flex: 1 }]} numberOfLines={1}>
                {held ? t('mobile.composer.release_to_send') : t('composer.recording')}
              </Text>
            </View>
          ) : (
            <TextInput
              ref={field}
              testID="composer-input"
              value={text}
              onChangeText={onChangeText}
              onSelectionChange={onSelectionChange}
              onFocus={() => {
                if (tool.type === 'attach') setTool(NONE);
              }}
              placeholder={placeholder}
              placeholderTextColor={palette.textMuted}
              selectionColor={palette.accent}
              accessibilityLabel={placeholder}
              multiline
              scrollEnabled
              textAlignVertical="top"
              style={[font.body, styles.input, { color: palette.text }]}
            />
          )}
        </View>
        <Pressable
          testID="composer-action"
          accessibilityRole="button"
          accessibilityLabel={action.label}
          accessibilityHint={
            action.icon === 'mic' && !recording ? t('mobile.composer.voice_hint') : undefined
          }
          accessibilityState={{ disabled: actionDisabled }}
          disabled={actionDisabled}
          onPress={onAction}
          onLongPress={onActionLongPress}
          onPressOut={() => voice.dispatch({ type: 'release', now: Date.now() })}
          delayLongPress={250}
          hitSlop={8}
          // A foreground ripple: Android's borderless one, drawn as the view's background, hid
          // the filled send button entirely.
          android_ripple={{ color: palette.hover, foreground: true }}
          style={[
            styles.action,
            { backgroundColor: action.background, opacity: actionDisabled ? 0.4 : 1 },
            held ? styles.held : null,
          ]}
        >
          <Icon
            name={action.icon}
            size={20}
            color={action.background === 'transparent' ? palette.textMuted : palette.onAccent}
          />
        </Pressable>
      </View>
      {tool.type === 'attach' && !recording ? <AttachTray onChoose={choose} /> : null}
    </View>
  );
}

/** The field's height for one line, and the most it grows to before it scrolls (about six lines). */
const FIELD_MIN = 36;

const FIELD_MAX = 140;

const styles = StyleSheet.create({
  composer: { borderTopWidth: StyleSheet.hairlineWidth, paddingTop: 6 },
  row: {
    flexDirection: 'row',
    alignItems: 'flex-end',
    gap: 8,
    paddingHorizontal: 8,
    paddingBottom: 6,
  },
  field: { flex: 1, minHeight: FIELD_MIN, borderRadius: FIELD_MIN / 2, justifyContent: 'center' },
  input: {
    minHeight: FIELD_MIN,
    maxHeight: FIELD_MAX,
    paddingHorizontal: 14,
    paddingTop: 8,
    paddingBottom: 8,
  },
  link: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 8,
    marginHorizontal: 12,
    marginBottom: 6,
    paddingLeft: 10,
    borderWidth: StyleSheet.hairlineWidth,
    borderRadius: 10,
  },
  linkInput: { flex: 1, paddingVertical: 8 },
  formatBar: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 6,
    paddingHorizontal: 8,
    paddingBottom: 6,
  },
  formats: { flex: 1 },
  recording: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 10,
    minHeight: FIELD_MIN,
    paddingHorizontal: 14,
  },
  dot: { width: 10, height: 10, borderRadius: 5 },
  action: {
    width: 36,
    height: 36,
    borderRadius: 18,
    alignItems: 'center',
    justifyContent: 'center',
    overflow: 'hidden',
  },
  held: { transform: [{ scale: 1.25 }] },
});
