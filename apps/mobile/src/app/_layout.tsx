// The root: the store, the theme and every route with how it is presented. Screens push on a
// native stack; actions open as native sheets (iOS form sheets, Android bottom sheets), and the
// call and attachment preview cover the screen. Signed out, only sign-in is reachable.

import {
  DarkTheme,
  DefaultTheme,
  type NativeStackNavigationOptions,
  Stack,
  ThemeProvider,
} from 'expo-router';
import * as SplashScreen from 'expo-splash-screen';
import { StatusBar } from 'expo-status-bar';
import * as Linking from 'expo-linking';
import { useEffect, useState } from 'react';
import { GestureHandlerRootView } from 'react-native-gesture-handler';
import { KeyboardProvider } from 'react-native-keyboard-controller';

import { bootDemo } from '../demo/boot';
import { demoScene } from '../demo/link';
import { DemoNavigation } from '../demo/navigation';
import type { SceneName } from '../demo/scenes';
import { boot } from '../engine/boot';
import { t } from '../i18n';
import { StoreProvider, useStore } from '../state/react';
import type { Store } from '../state/store';
import { VerificationPrompt } from '../ui/accounts/VerificationPrompt';
import { Rings } from '../ui/call/Rings';
import { LinkOpener } from '../ui/links/LinkOpener';
import { Notices } from '../ui/Notices';
import { headerTint, usePalette } from '../ui/theme';

void SplashScreen.preventAutoHideAsync();

/** Sheets for actions: half height first, full on a drag, like Slack's message actions. */
const sheet: NativeStackNavigationOptions = {
  presentation: 'formSheet',
  sheetAllowedDetents: [0.5, 1],
  sheetGrabberVisible: true,
  sheetCornerRadius: 16,
  headerShown: false,
};

/** Sheets that fit their content (menus). */
const fittedSheet: NativeStackNavigationOptions = {
  ...sheet,
  sheetAllowedDetents: 'fitToContents',
};

function Routes({ scene }: { scene: SceneName | null }) {
  const palette = usePalette();
  const started = useStore((s) => s.started);
  const restoring = useStore((s) => s.restoring.length > 0);
  const signedIn = useStore((s) => s.accounts.length > 0);
  // Launch is over once the engine said what it restores and the first account (if any) is back.
  const ready = started && (signedIn || !restoring);
  useEffect(() => {
    if (ready) void SplashScreen.hideAsync();
  }, [ready]);
  const base = palette.dark ? DarkTheme : DefaultTheme;

  const theme = {
    ...base,
    colors: {
      ...base.colors,
      primary: palette.accent,
      background: palette.bg,
      card: palette.bg,
      text: palette.text,
      border: palette.divider,
      notification: palette.mention,
    },
  };

  if (!ready) return null;

  return (
    <ThemeProvider value={theme}>
      <StatusBar style={palette.dark ? 'light' : 'dark'} />
      <Stack
        screenOptions={{
          headerTintColor: headerTint(palette),
          headerTitleStyle: { color: palette.text },
          headerStyle: { backgroundColor: palette.bg },
          contentStyle: { backgroundColor: palette.bg },
          headerBackButtonDisplayMode: 'minimal',
        }}
      >
        <Stack.Protected guard={signedIn}>
          <Stack.Screen name="(tabs)" options={{ headerShown: false, title: t('app.name') }} />
          <Stack.Screen name="room/[account]/[roomId]" />
          <Stack.Screen name="thread/[account]/[roomId]/[root]" />
          <Stack.Screen name="room-info/[account]/[roomId]" />
          <Stack.Screen name="account-menu" options={fittedSheet} />
          <Stack.Screen name="create" options={fittedSheet} />
          <Stack.Screen name="spaces" options={sheet} />
          <Stack.Screen name="room-menu" options={fittedSheet} />
          <Stack.Screen name="invitation" options={fittedSheet} />
          <Stack.Screen name="message-actions" options={sheet} />
          <Stack.Screen name="attachment-actions" options={fittedSheet} />
          <Stack.Screen name="forward-message" options={{ presentation: 'modal' }} />
          <Stack.Screen name="pinned-messages" />
          <Stack.Screen name="emoji" options={sheet} />
          <Stack.Screen name="member" options={fittedSheet} />
          <Stack.Screen name="readers" options={sheet} />
          <Stack.Screen name="link" options={fittedSheet} />
          <Stack.Screen name="new-poll" options={{ presentation: 'modal' }} />
          <Stack.Screen name="new-chat" options={{ presentation: 'modal' }} />
          <Stack.Screen name="new-room" options={{ presentation: 'modal' }} />
          <Stack.Screen name="new-space" options={{ presentation: 'modal' }} />
          <Stack.Screen name="rename-room" options={{ presentation: 'modal' }} />
          <Stack.Screen name="invite" options={{ presentation: 'modal' }} />
          <Stack.Screen name="settings" options={{ headerShown: false }} />
          <Stack.Screen name="verify" options={{ ...sheet, sheetAllowedDetents: [0.75, 1] }} />
          <Stack.Screen name="recovery" options={{ presentation: 'modal' }} />
          <Stack.Screen
            name="preview"
            options={{ presentation: 'fullScreenModal', headerShown: false, animation: 'fade' }}
          />
          <Stack.Screen
            name="call"
            options={{ presentation: 'fullScreenModal', headerShown: false }}
          />
        </Stack.Protected>
        <Stack.Screen
          name="sign-in"
          options={{
            headerShown: false,
            presentation: signedIn ? 'fullScreenModal' : 'card',
            animation: signedIn ? 'default' : 'none',
          }}
        />
      </Stack>
      <Rings />
      <VerificationPrompt />
      <Notices />
      <LinkOpener />
      {scene ? <DemoNavigation scene={scene} /> : null}
    </ThemeProvider>
  );
}

interface App {
  store: Store;
  /** The demo scene on screen, in a dev build opened with `bettermatrix://demo/<scene>`. */
  scene: SceneName | null;
  /** Bumped per scene, so a new scene starts from fresh screens. */
  generation: number;
}

function launch(): App {
  const scene = __DEV__ ? demoScene(Linking.getLinkingURL()) : null;

  return scene
    ? { store: bootDemo(scene), scene, generation: 0 }
    : { store: boot(), scene: null, generation: 0 };
}

export default function RootLayout() {
  const [app, setApp] = useState(launch);
  // Dev builds switch scenes while running: `adb shell am start -d bettermatrix://demo/<scene>`.
  useEffect(() => {
    if (!__DEV__) return;

    const subscription = Linking.addEventListener('url', ({ url }) => {
      const scene = demoScene(url);

      if (scene)
        setApp((current) => ({
          store: bootDemo(scene),
          scene,
          generation: current.generation + 1,
        }));
    });

    return () => subscription.remove();
  }, []);

  return (
    <GestureHandlerRootView style={{ flex: 1 }}>
      <KeyboardProvider>
        <StoreProvider store={app.store}>
          <Routes key={app.generation} scene={app.scene} />
        </StoreProvider>
      </KeyboardProvider>
    </GestureHandlerRootView>
  );
}
