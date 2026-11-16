// The compact stage over a room's chat while that room's call runs and the user chose its chat
// (the phone's form of the desktop's chat below a compact stage): one surface at a time for a
// video call (swipe or the arrows page through every participant and shared screen), a row of
// faces for a voice call, and small controls. Tapping it, or the expand control, opens the full
// stage again (ADR 0010).

import type { AccountId, CallInfo } from '@better-matrix/matrix-native/types';
import { StyleSheet, useWindowDimensions, View } from 'react-native';

import { useStore } from '../../state/react';
import { usePalette } from '../theme';
import { CallControls } from './CallControls';
import { CallTiles, Paging, useCallPlan } from './CallTiles';
import { showsVideo } from './stage';
import { useCallAudio } from './useCallAudio';
import { useOpenStage } from './useOpenStage';

/** The height of a voice call's row of faces: one face of 104×80 at least, with its name. */
const VOICE = 120;

export function CompactCall({ account, roomId }: { account: AccountId; roomId: string }) {
  const info = useStore((s) => {
    const current = s.call.current;
    return current && current.account === account && current.room_id === roomId && s.call.view.chat ? current : null;
  });
  if (!info) return null;
  return <Compact info={info} />;
}

function Compact({ info }: { info: CallInfo }) {
  const palette = usePalette();
  const { width } = useWindowDimensions();
  const view = useStore((s) => s.call.view);
  const heard = useCallAudio();
  const { planned, size, onLayout, preview } = useCallPlan(info, view, heard.speaker, true);
  const video = showsVideo(info);
  // A 16:9 surface across the phone, kept short enough to leave the chat its room.
  const height = video ? Math.min(Math.max(((width - 24) * 9) / 16, 150), 220) : VOICE;
  const expand = useOpenStage();
  return (
    <View testID="call-compact" style={[styles.compact, { backgroundColor: palette.surface, borderColor: palette.divider }]}>
      <View style={{ height }}>
        <CallTiles
          info={info}
          view={view}
          planned={planned}
          size={size}
          heard={heard}
          compact
          onLayout={onLayout}
          preview={preview}
          onTilePress={expand}
        />
      </View>
      <View style={styles.bar}>
        <Paging planned={planned} compact />
        <View style={styles.controls}>
          <CallControls info={info} compact onExpand={expand} />
        </View>
      </View>
    </View>
  );
}

const styles = StyleSheet.create({
  compact: { paddingHorizontal: 12, paddingTop: 10, paddingBottom: 8, gap: 8, borderBottomWidth: StyleSheet.hairlineWidth },
  bar: { flexDirection: 'row', alignItems: 'center', justifyContent: 'space-between', gap: 8 },
  controls: { flex: 1, alignItems: 'flex-end' },
});
