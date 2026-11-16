// Calls ringing for the user, over every screen (the desktop's ring cards), and what the app does
// around any call: the ring sound and a buzz while one rings, opening the stage when a call
// starts or is answered, the phone's call audio while one runs, and the notices calls give.
// Mounted once at the root.

import type { IncomingCall } from '@better-matrix/matrix-native/types';
import { createAudioPlayer } from 'expo-audio';
import * as Haptics from 'expo-haptics';
import { usePathname, useRouter } from 'expo-router';
import { useEffect, useRef } from 'react';
import { Pressable, StyleSheet, Text, View } from 'react-native';
import Animated, { FadeInUp, FadeOutUp } from 'react-native-reanimated';
import { useSafeAreaInsets } from 'react-native-safe-area-context';

import { setCallAudio } from '../../engine/callMedia';
import { t } from '../../i18n';
import { answerCall, callEffect, declineCall } from '../../state/actions/calls';
import { shallow, useAppStore, useStore } from '../../state/react';
import { routes } from '../../routes';
import { font } from '../../theme/palette';
import { Avatar } from '../Avatar';
import { Icon, type IconName } from '../icons';
import { usePalette } from '../theme';
import { ringLabel } from './stage';

const RING_SOUND = require('../../../assets/sounds/ring.wav') as number;

export function Rings() {
  const rings = useStore((s) => s.call.rings, shallow);
  const insets = useSafeAreaInsets();
  return (
    <>
      <CallEffects />
      {rings.length > 0 ? (
        <View style={[styles.stack, { top: insets.top + 8 }]} pointerEvents="box-none">
          {rings.map((ring) => (
            <RingCard key={`${ring.account}|${ring.room_id}`} ring={ring} />
          ))}
        </View>
      ) : null}
    </>
  );
}

function RingCard({ ring }: { ring: IncomingCall }) {
  const store = useAppStore();
  const palette = usePalette();
  const id = `${ring.account}-${ring.room_id}`;
  return (
    <Animated.View
      entering={FadeInUp}
      exiting={FadeOutUp}
      testID={`ring-${id}`}
      accessibilityRole="alert"
      accessibilityLabel={`${ring.caller.name}, ${ringLabel(ring)}`}
      style={[styles.card, { backgroundColor: palette.surface, borderColor: palette.divider }]}
    >
      <View style={[styles.ringed, { borderColor: palette.accent, shadowColor: palette.accent }]}>
        <Avatar account={ring.account} id={ring.caller.id} name={ring.caller.name} media={ring.caller.avatar} size={44} />
      </View>
      <View style={styles.who}>
        <Text style={[font.bodyStrong, { color: palette.text }]} numberOfLines={1}>
          {ring.caller.name}
        </Text>
        <Text style={[font.meta, { color: palette.textMuted }]} numberOfLines={1}>
          {ringLabel(ring)}
        </Text>
      </View>
      <RoundButton
        testID={`ring-decline-${id}`}
        icon="hangUp"
        label={t('call.decline')}
        color={palette.danger}
        onPress={() => void declineCall(store, ring.account, ring.room_id)}
      />
      {ring.video ? (
        <RoundButton
          testID={`ring-answer-video-${id}`}
          icon="video"
          label={t('call.answer_video')}
          color={palette.success}
          onPress={() => void answerCall(store, ring.account, ring.room_id, true)}
        />
      ) : null}
      <RoundButton
        testID={`ring-answer-${id}`}
        icon="call"
        label={t('call.answer')}
        color={palette.success}
        onPress={() => void answerCall(store, ring.account, ring.room_id, false)}
      />
    </Animated.View>
  );
}

function RoundButton({ icon, label, color, onPress, testID }: { icon: IconName; label: string; color: string; onPress: () => void; testID: string }) {
  return (
    <Pressable
      testID={testID}
      onPress={onPress}
      accessibilityRole="button"
      accessibilityLabel={label}
      hitSlop={4}
      style={({ pressed }) => [styles.round, { backgroundColor: color, opacity: pressed ? 0.8 : 1 }]}
    >
      <Icon name={icon} size={22} color="#ffffff" />
    </Pressable>
  );
}

/** What happens around calls, outside any one screen. */
function CallEffects() {
  const store = useAppStore();
  const router = useRouter();
  const pathname = usePathname();
  const call = useStore((s) => s.call.current?.call ?? null);
  const video = useStore((s) => s.call.current?.video ?? false);
  const ringCount = useStore((s) => s.call.rings.length);
  const ringSound = useStore((s) => s.settings.ringSound);

  // Calls' notices: why a call ended, a camera that failed or stopped.
  useEffect(() => store.addEffect(callEffect), [store]);

  // A call that starts or is answered opens its stage, once.
  const opened = useRef<number | null>(null);
  useEffect(() => {
    if (call === null || opened.current === call) return;
    opened.current = call;
    if (pathname !== '/call') router.push(routes.call());
  }, [call, pathname, router]);

  // The phone's call audio while a call runs; a video call starts on the loudspeaker.
  const inCall = call !== null;
  useEffect(() => {
    setCallAudio(inCall, video);
    // Only a call starting or ending changes the route, not its camera.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [inCall]);

  // A buzz for each new ring.
  const rung = useRef(0);
  useEffect(() => {
    if (ringCount > rung.current) void Haptics.notificationAsync(Haptics.NotificationFeedbackType.Warning);
    rung.current = ringCount;
  }, [ringCount]);

  // The ring sound while something rings, unless the user made rings silent.
  const ringing = ringSound && ringCount > 0;
  useEffect(() => {
    if (!ringing) return;
    const player = createAudioPlayer(RING_SOUND);
    player.loop = true;
    player.play();
    return () => player.remove();
  }, [ringing]);

  return null;
}

const styles = StyleSheet.create({
  stack: { position: 'absolute', left: 12, right: 12, gap: 10, zIndex: 90 },
  card: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 10,
    padding: 12,
    borderRadius: 14,
    borderWidth: StyleSheet.hairlineWidth,
    shadowColor: '#000',
    shadowOpacity: 0.3,
    shadowRadius: 14,
    shadowOffset: { width: 0, height: 6 },
    elevation: 8,
  },
  ringed: { borderWidth: 2, borderRadius: 26, padding: 2, shadowOpacity: 0.55, shadowRadius: 11, shadowOffset: { width: 0, height: 0 } },
  who: { flex: 1, minWidth: 0 },
  round: { width: 44, height: 44, borderRadius: 22, alignItems: 'center', justifyContent: 'center' },
});
