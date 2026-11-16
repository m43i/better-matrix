// The rows of a room's details beside its members: its threads and its shared files, as the
// desktop's info panels list them.

import type { AttachmentKind, SharedFile, ThreadInfo } from '@better-matrix/matrix-native/types';
import { ActivityIndicator, StyleSheet, Text, View } from 'react-native';

import { tp } from '../../i18n';
import { font } from '../../theme/palette';
import { Touchable } from '../components';
import { Icon, type IconName } from '../icons';
import { usePalette } from '../theme';
import { when } from './labels';

/** A thread: who started it, its first message, its replies and when the last came. */
export function ThreadRow({ thread, onPress }: { thread: ThreadInfo; onPress: () => void }) {
  const palette = usePalette();
  const replies = tp('thread.replies', thread.replies);
  const meta = thread.latest_at === null ? replies : `${replies} · ${when(thread.latest_at)}`;

  return (
    <Touchable
      testID={`listed-thread-${thread.root}`}
      onPress={onPress}
      accessibilityRole="button"
      accessibilityLabel={thread.body}
    >
      <View style={styles.thread}>
        <Text style={[font.bodyStrong, { color: palette.text }]} numberOfLines={1}>
          {thread.sender_name}
        </Text>
        <Text style={[font.body, { color: palette.text }]} numberOfLines={2}>
          {thread.body}
        </Text>
        <Text style={[font.meta, { color: palette.accent }]}>{meta}</Text>
      </View>
    </Touchable>
  );
}

const FILE_ICONS: Record<AttachmentKind, IconName> = {
  Image: 'image',
  Video: 'video',
  Audio: 'audio',
  File: 'file',
};

/** A shared file: its kind, name, who shared it and when. */
export function FileRow({
  file,
  busy,
  onPress,
}: {
  file: SharedFile;
  busy: boolean;
  onPress: () => void;
}) {
  const palette = usePalette();

  return (
    <Touchable
      testID={`listed-file-${file.event_id}`}
      onPress={onPress}
      accessibilityRole="button"
      accessibilityLabel={file.attachment.name}
    >
      <View style={styles.file}>
        <View
          style={[styles.kind, { backgroundColor: palette.surface, borderColor: palette.divider }]}
        >
          <Icon name={FILE_ICONS[file.attachment.kind]} size={20} color={palette.accent} />
        </View>
        <View style={styles.fileText}>
          <Text style={[font.body, { color: palette.text }]} numberOfLines={1}>
            {file.attachment.name}
          </Text>
          <Text style={[font.meta, { color: palette.textMuted }]} numberOfLines={1}>
            {`${file.sender_name} · ${when(file.timestamp)}`}
          </Text>
        </View>
        {busy ? <ActivityIndicator color={palette.textMuted} /> : null}
      </View>
    </Touchable>
  );
}

const styles = StyleSheet.create({
  thread: { paddingHorizontal: 16, paddingVertical: 10, gap: 3 },
  file: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 12,
    paddingHorizontal: 16,
    paddingVertical: 10,
  },
  kind: {
    width: 40,
    height: 40,
    borderRadius: 10,
    alignItems: 'center',
    justifyContent: 'center',
    borderWidth: StyleSheet.hairlineWidth,
  },
  fileText: { flex: 1, minWidth: 0, gap: 2 },
});
