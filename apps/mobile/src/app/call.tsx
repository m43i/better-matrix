// The call's stage, full screen (the desktop's `ui::call::stage`): the native header with the
// room's name, the way back and the call layout menu (Auto, Grid, Focus, self-view), the line
// saying where the call stands, the call plan (faces for a voice call; a gallery, a one-to-one
// view or a focused surface with the participant strip for video), and the controls. Chat shows
// the room's conversation under a compact stage (`ui/call/CompactCall`). Closing it leaves the
// call running behind the call bar; the stage closes itself when the call ends.

import type { CallInfo } from '@better-matrix/matrix-native/types';
import { Stack, useRouter } from 'expo-router';
import { useEffect } from 'react';
import { StyleSheet, Text, View } from 'react-native';
import { SafeAreaView } from 'react-native-safe-area-context';

import { type Key, t } from '../i18n';
import { routes } from '../routes';
import { setCallLayout, showCallChat, toggleCallSelf } from '../state/actions/calls';
import type { CallLayout } from '../state/callLayout';
import type { CallView } from '../state/callView';
import { useAppStore, useStore } from '../state/react';
import { room as roomOf } from '../state/selectors';
import { font } from '../theme/palette';
import { CallControls } from '../ui/call/CallControls';
import { CallTiles, Paging, useCallPlan } from '../ui/call/CallTiles';
import { showsVideo, stageStatus } from '../ui/call/stage';
import { useCallAudio } from '../ui/call/useCallAudio';
import { useToolbarIcon } from '../ui/shell/toolbarIcon';
import { usePalette } from '../ui/theme';

export default function CallScreen() {
  const router = useRouter();
  const palette = usePalette();
  const call = useStore((s) => s.call.current);
  // The call ended (or was never there): the stage goes.
  useEffect(() => {
    if (!call && router.canGoBack()) router.back();
  }, [call, router]);

  if (!call) return <View style={{ flex: 1, backgroundColor: palette.bg }} />;

  return <Stage info={call} />;
}

function Stage({ info }: { info: CallInfo }) {
  const store = useAppStore();
  const router = useRouter();
  const palette = usePalette();
  const view = useStore((s) => s.call.view);
  const roomName = useStore((s) => roomOf(s, info.account, info.room_id)?.name ?? '');
  const heard = useCallAudio();
  const { planned, size, onLayout, preview } = useCallPlan(info, view, heard.speaker, false);
  const status = stageStatus(info);

  const chat = () => {
    showCallChat(store, true);
    router.dismissTo(routes.room(info.account, info.room_id));
  };

  return (
    <>
      <Stack.Screen
        options={{
          headerShown: true,
          title: roomName,
          headerTintColor: palette.text,
          headerStyle: { backgroundColor: palette.bg },
          headerShadowVisible: false,
        }}
      />
      <Header view={view} video={showsVideo(info)} onClose={() => router.back()} />
      <SafeAreaView
        edges={['left', 'right', 'bottom']}
        style={[styles.screen, { backgroundColor: palette.bg }]}
        testID="call-stage"
      >
        <Glow />
        {status ? (
          <Text
            testID="call-status"
            style={[font.body, styles.status, { color: palette.textMuted }]}
            numberOfLines={1}
          >
            {status}
          </Text>
        ) : null}
        <View style={styles.stage}>
          <CallTiles
            info={info}
            view={view}
            planned={planned}
            size={size}
            heard={heard}
            compact={false}
            onLayout={onLayout}
            preview={preview}
          />
        </View>
        <Paging planned={planned} />
        <CallControls info={info} onChat={chat} />
      </SafeAreaView>
    </>
  );
}

const LAYOUTS: ReadonlyArray<{ layout: CallLayout; label: Key; testID: string }> = [
  { layout: { type: 'Auto' }, label: 'call.layout_auto', testID: 'call-layout-auto' },
  { layout: { type: 'Grid' }, label: 'call.layout_grid', testID: 'call-layout-grid' },
  { layout: { type: 'Focus', pin: null }, label: 'call.layout_focus', testID: 'call-layout-focus' },
];

/**
 * The header's buttons: back to where the user was (the call keeps running), and the call layout
 * menu: Auto, Grid and Focus (following the speaker) for a video call, the current one checked;
 * hide or show self-view.
 */
function Header({ view, video, onClose }: { view: CallView; video: boolean; onClose: () => void }) {
  const store = useAppStore();
  const palette = usePalette();
  const down = useToolbarIcon('chevronDown', palette.text);
  const layout = useToolbarIcon('layout', palette.text);
  const self = useToolbarIcon(view.showSelf ? 'eyeOff' : 'eye', palette.text);

  return (
    <>
      <Stack.Toolbar placement="left">
        <Stack.Toolbar.Button
          icon={down}
          accessibilityLabel={t('mobile.calls.minimize')}
          onPress={onClose}
        />
      </Stack.Toolbar>
      <Stack.Toolbar placement="right">
        <Stack.Toolbar.Menu
          icon={layout}
          accessibilityLabel={t('call.layout')}
          title={t('call.layout')}
        >
          <Stack.Toolbar.Menu inline hidden={!video} title={t('call.layout')}>
            {LAYOUTS.map((choice) => (
              <Stack.Toolbar.MenuAction
                key={choice.testID}
                isOn={choice.layout.type === view.layout.type}
                subtitle={choice.layout.type === 'Focus' ? t('call.follow_speaker') : undefined}
                onPress={() => setCallLayout(store, choice.layout)}
              >
                {t(choice.label)}
              </Stack.Toolbar.MenuAction>
            ))}
          </Stack.Toolbar.Menu>
          <Stack.Toolbar.MenuAction icon={self} onPress={() => toggleCallSelf(store)}>
            {view.showSelf ? t('call.hide_self') : t('call.show_self')}
          </Stack.Toolbar.MenuAction>
        </Stack.Toolbar.Menu>
      </Stack.Toolbar>
    </>
  );
}

/** A soft wash of the accent behind the stage that swells with everyone's voices. */
function Glow() {
  const palette = usePalette();

  // Steps of 16, taken in the selector, keep the stage from redrawing on every level report.
  const steps = useStore((s) =>
    Math.round(Object.values(s.call.levels).reduce((max, level) => Math.max(max, level), 0) / 16),
  );

  const step = steps / 16;

  return (
    <View
      pointerEvents="none"
      style={[
        StyleSheet.absoluteFill,
        { backgroundColor: palette.accent, opacity: 0.04 + step * 0.14 },
      ]}
    />
  );
}

const styles = StyleSheet.create({
  screen: { flex: 1 },
  status: { textAlign: 'center', paddingHorizontal: 16, paddingTop: 4 },
  stage: { flex: 1, paddingHorizontal: 12, paddingTop: 12 },
});
