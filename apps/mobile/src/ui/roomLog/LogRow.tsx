// One line of the room log: who made the change, what the timeline says of it, and when.

import { memo } from 'react';
import { StyleSheet, Text, View } from 'react-native';

import type { LogEntry } from '../../state/roomLog';
import { roomEventText } from '../../state/timelineText';
import { font } from '../../theme/palette';
import { Avatar } from '../Avatar';
import { when } from '../people/labels';
import { usePalette } from '../theme';

export const LogRow = memo(LogRowView);

function LogRowView({ account, entry }: { account: string; entry: LogEntry }) {
  const palette = usePalette();
  const { sender, timestamp_ms } = entry.event;

  return (
    <View testID={`log-${entry.id}`} style={styles.row}>
      <Avatar account={account} id={sender.id} name={sender.name} media={sender.avatar} size={32} />
      <View style={styles.text}>
        <Text style={[font.body, { color: palette.text }]}>{roomEventText(entry.event)}</Text>
        <Text style={[font.meta, { color: palette.textMuted }]}>{when(timestamp_ms)}</Text>
      </View>
    </View>
  );
}

const styles = StyleSheet.create({
  row: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 12,
    paddingHorizontal: 16,
    paddingVertical: 10,
  },
  text: { flex: 1, gap: 2 },
});
