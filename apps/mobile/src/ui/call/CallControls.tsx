// The call's round controls (the desktop's `ui::call::controls`): microphone, camera (and
// switching it), speaker, chat, hang up on the full stage; the compact stage over the chat keeps
// the microphone, camera and hang up small, and leads back to the full stage.

import type { CallInfo } from '@better-matrix/matrix-native/types';
import { useState } from 'react';
import { Pressable, StyleSheet, View } from 'react-native';

import {
  canRouteAudio,
  canShowVideo,
  cameraNames,
  setSpeaker,
  useSpeaker,
} from '../../engine/callMedia';
import { t } from '../../i18n';
import { hangUp, switchCamera, toggleCamera, toggleMicrophone } from '../../state/actions/calls';
import { useAppStore } from '../../state/react';
import { Icon, type IconName } from '../icons';
import { usePalette } from '../theme';
import { controls } from './stage';

/** The size of a round control on the full stage, and on the compact one. */
const CONTROL = 50;

const SMALL = 36;

export function CallControls({
  info,
  compact = false,
  onChat,
  onExpand,
}: {
  info: CallInfo;
  compact?: boolean;
  /** Show the room's chat (the full stage). */
  onChat?: () => void;
  /** Back to the full stage (the compact one). */
  onExpand?: () => void;
}) {
  const store = useAppStore();
  const palette = usePalette();
  const speaker = useSpeaker();
  const { microphone, camera } = controls(info, canShowVideo());
  const [cameras] = useState(cameraNames);
  const size = compact ? SMALL : CONTROL;

  return (
    <View style={[styles.controls, compact ? styles.compact : null]} testID="call-controls">
      <Control
        testID="call-mic"
        icon={microphone.on ? 'mic' : 'micOff'}
        label={microphone.label}
        on={microphone.on}
        size={size}
        onPress={() => void toggleMicrophone(store)}
      />
      {camera.shown ? (
        <Control
          testID="call-camera"
          icon={camera.on ? 'video' : 'videoOff'}
          label={camera.label}
          on={camera.on}
          size={size}
          onPress={() => void toggleCamera(store)}
        />
      ) : null}
      {!compact && camera.on && cameras.length > 1 ? (
        <Control
          testID="call-flip"
          icon="flipCamera"
          label={t('mobile.calls.flip_camera')}
          on
          size={size}
          onPress={() => void switchCamera(store)}
        />
      ) : null}
      {!compact && canRouteAudio() ? (
        <Control
          testID="call-speaker"
          icon="speaker"
          label={speaker ? t('mobile.calls.speaker_off') : t('mobile.calls.speaker_on')}
          on={!speaker}
          size={size}
          onPress={() => setSpeaker(!speaker)}
        />
      ) : null}
      {onChat ? (
        <Control
          testID="call-chat"
          icon="chat"
          label={t('call.show_chat')}
          on
          size={size}
          onPress={onChat}
        />
      ) : null}
      {onExpand ? (
        <Control
          testID="call-back"
          icon="expand"
          label={t('call.show_stage')}
          on
          size={size}
          onPress={onExpand}
        />
      ) : null}
      <Pressable
        testID="call-hang-up"
        accessibilityRole="button"
        accessibilityLabel={t('call.hang_up')}
        onPress={() => void hangUp(store)}
        style={({ pressed }) => [
          round(size),
          { backgroundColor: palette.danger, opacity: pressed ? 0.8 : 1 },
        ]}
      >
        <Icon name="hangUp" size={size * 0.48} color="#ffffff" />
      </Pressable>
    </View>
  );
}

/** A round toggle: light while on, inverted while off (the desktop's call controls). */
function Control({
  icon,
  label,
  on,
  size,
  onPress,
  testID,
}: {
  icon: IconName;
  label: string;
  on: boolean;
  size: number;
  onPress: () => void;
  testID: string;
}) {
  const palette = usePalette();

  return (
    <Pressable
      testID={testID}
      accessibilityRole="button"
      accessibilityLabel={label}
      accessibilityState={{ selected: !on }}
      onPress={onPress}
      style={({ pressed }) => [
        round(size),
        {
          backgroundColor: on ? palette.surface : palette.text,
          borderColor: palette.divider,
          opacity: pressed ? 0.8 : 1,
        },
      ]}
    >
      <Icon name={icon} size={size * 0.44} color={on ? palette.text : palette.bg} />
    </Pressable>
  );
}

function round(size: number) {
  return {
    width: size,
    height: size,
    borderRadius: size / 2,
    alignItems: 'center',
    justifyContent: 'center',
    borderWidth: StyleSheet.hairlineWidth,
  } as const;
}

const styles = StyleSheet.create({
  controls: {
    flexDirection: 'row',
    flexWrap: 'wrap',
    justifyContent: 'center',
    alignItems: 'center',
    gap: 10,
    paddingVertical: 16,
    paddingHorizontal: 8,
  },
  compact: { flexWrap: 'nowrap', gap: 8, paddingVertical: 0, paddingHorizontal: 0 },
});
