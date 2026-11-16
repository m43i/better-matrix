// Recording a voice memo with the platform's recorder (expo-audio): AAC in `.m4a`, mono, sent as
// a Matrix voice message with its length. Runs the effects `voiceStep` asks for.

import {
  type RecordingOptions,
  RecordingPresets,
  requestRecordingPermissionsAsync,
  setAudioModeAsync,
  useAudioRecorder,
} from 'expo-audio';
import * as Haptics from 'expo-haptics';
import { useCallback, useEffect, useRef, useState } from 'react';
import { Alert } from 'react-native';

import { t } from '../../i18n';
import { pathFromUri } from '../../state/outgoing';
import { VOICE_IDLE, type VoiceEvent, type VoiceMemo, voiceStep } from '../../state/voiceMemo';

/** Speech needs neither stereo nor music's bit rate. */
const VOICE: RecordingOptions = { ...RecordingPresets.HIGH_QUALITY, numberOfChannels: 1, bitRate: 64_000 };

export interface VoiceMemoControls {
  state: VoiceMemo;
  /** How long it has recorded, in ms. */
  elapsed: number;
  dispatch: (event: VoiceEvent) => void;
}

/** `onSend` gets the recording's path and length once a memo is to be sent. */
export function useVoiceMemo(onSend: (path: string, durationMs: number) => void, onError: (message: string) => void): VoiceMemoControls {
  const recorder = useAudioRecorder(VOICE);
  const machine = useRef<VoiceMemo>(VOICE_IDLE);
  const [state, setState] = useState<VoiceMemo>(VOICE_IDLE);
  const [elapsed, setElapsed] = useState(0);
  const handlers = useRef({ onSend, onError });
  useEffect(() => {
    handlers.current = { onSend, onError };
  });

  const dispatch = useCallback(
    (event: VoiceEvent) => {
      const before = machine.current;
      const { next, effect } = voiceStep(before, event);
      machine.current = next;
      setState(next);
      if (effect === 'start') {
        void start(event.type === 'press' && event.held);
      } else if (effect === 'send' || effect === 'discard') {
        const duration = before.phase === 'recording' ? Date.now() - before.startedAt : 0;
        void stop(effect === 'send', duration);
      }

      async function start(held: boolean): Promise<void> {
        try {
          const permission = await requestRecordingPermissionsAsync();
          if (!permission.granted) {
            dispatch({ type: 'failed' });
            Alert.alert(t('composer.voice'), t('mobile.composer.microphone_denied'));
            return;
          }
          await setAudioModeAsync({ allowsRecording: true, playsInSilentMode: true });
          await recorder.prepareToRecordAsync();
          recorder.record();
          if (held) void Haptics.impactAsync(Haptics.ImpactFeedbackStyle.Medium);
          dispatch({ type: 'started', now: Date.now() });
        } catch (error) {
          dispatch({ type: 'failed' });
          handlers.current.onError(t('notice.voice', { error: error instanceof Error ? error.message : String(error) }));
        }
      }

      async function stop(send: boolean, durationMs: number): Promise<void> {
        try {
          await recorder.stop();
          await setAudioModeAsync({ allowsRecording: false });
          const uri = recorder.uri;
          if (send && uri) handlers.current.onSend(pathFromUri(uri), durationMs);
        } catch (error) {
          handlers.current.onError(t('notice.voice', { error: error instanceof Error ? error.message : String(error) }));
        } finally {
          dispatch({ type: 'stopped' });
        }
      }
    },
    [recorder],
  );

  // The clock while recording.
  useEffect(() => {
    if (state.phase !== 'recording') {
      setElapsed(0);
      return;
    }
    const started = state.startedAt;
    const timer = setInterval(() => setElapsed(Date.now() - started), 250);
    return () => clearInterval(timer);
  }, [state]);

  // A memo still recording when the composer goes is dropped, as on the desktop.
  useEffect(
    () => () => {
      if (machine.current.phase === 'recording' || machine.current.phase === 'starting') {
        recorder.stop().catch(() => undefined);
        setAudioModeAsync({ allowsRecording: false }).catch(() => undefined);
      }
    },
    [recorder],
  );

  return { state, elapsed, dispatch };
}
