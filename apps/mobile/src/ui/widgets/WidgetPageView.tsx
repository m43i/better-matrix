// A running widget's page in a web view (ADR 0010). The page's main frame runs the run's script
// (`bridgeScript`), which keeps the user's location from it and, for a page with a bridge (with
// the run's token), hands the app what the page posts for the engine and when its document
// starts and finishes loading; the app takes it only with that token and from the page's origin.
// The web view reports its main frame's address to the run (`WidgetRun`), which posts the
// engine's messages, for the page's origin only, while a loaded document has exactly that
// origin; the posting script checks the origin itself as well. A page with a bridge stays on its
// origin: a top-frame navigation elsewhere opens in the system's browser (`navigation`); a
// meeting may go anywhere on the web. Windows a page opens go to the browser.
//
// Only a meeting (no bridge) uses the camera and microphone (`captureGrant`), media playing
// inline without a tap: iOS asks for each site itself (`prompt`); Android's web view would grant
// any page the app's permissions, so react-native-webview is patched
// (patches/react-native-webview@14.0.1.patch) to take the approved host (`mediaCaptureHost`) and
// grant capture under `grantIfSameHostElseDeny` only to pages on it (checked when asked and again
// after the system's dialog), else to deny it without asking the system. Any other widget gets
// `deny` on both, and the bridge script also makes `getUserMedia` and `getDisplayMedia` fail in
// its main frame. No widget gets the user's location (`geolocationEnabled` off on Android, the
// script on both; frames inside a page on iOS still meet WebKit's own prompt). A load or HTTP
// failure of the page ends the run, logged without its address.

import type { WidgetPage } from '@better-matrix/matrix-native/types';
import { useEffect, useRef } from 'react';
import { Linking, Platform, StyleSheet } from 'react-native';
import WebView from 'react-native-webview';

import type { WidgetRun } from '../../state/actions/widgets';
import {
  bridgeScript,
  captureGrant,
  httpFailure,
  loadFailure,
  navigation,
  originOf,
  postScript,
} from '../../state/widgets';
import { usePalette } from '../theme';
import { PAGE_SETTINGS } from './pageSettings';

/** Opens a web page in the system's browser; anything but `http(s)` is left alone. */
function openOutside(url: string): void {
  if (/^https?:\/\//i.test(url)) Linking.openURL(url).catch(() => {});
}

/** Notes why a page failed, in development: the reason holds no address (`loadFailure`). */
function logFailure(reason: string): void {
  if (__DEV__) console.warn('widget page failed:', reason);
}

export function WidgetPageView({
  run,
  page,
  captureHost,
}: {
  run: WidgetRun;
  page: WidgetPage;
  /** The host a meeting may use the camera and microphone on (`WidgetRun`'s `captureHost`). */
  captureHost: string | null;
}) {
  const palette = usePalette();
  // The typings default the class's extra props to `undefined`, which leaves it no props at all.
  const view = useRef<WebView<object>>(null);
  const script = bridgeScript(page.bridge, run.token);
  const origin = originOf(page.url);
  const grant = captureGrant(Platform.OS === 'ios' ? 'ios' : 'android', captureHost);

  useEffect(() => {
    run.attach((message) => {
      // A page with no origin (none on the web), or a message that isn't an object, gets nothing.
      const posting = origin === null ? null : postScript(message, origin);

      if (posting !== null) view.current?.injectJavaScript(posting);
    });

    return () => run.attach(null);
  }, [run, origin]);

  return (
    <WebView<object>
      ref={view}
      testID="widget-page"
      source={{ uri: page.url }}
      style={[styles.page, { backgroundColor: palette.bg }]}
      {...PAGE_SETTINGS}
      injectedJavaScriptBeforeContentLoaded={script}
      onMessage={(event) => run.fromPage(event.nativeEvent.data, event.nativeEvent.url)}
      onLoadStart={(event) => run.pageStarted(event.nativeEvent.url)}
      onLoadEnd={(event) => {
        // The script didn't run in this document (Android may skip it): install it now.
        if (run.pageEnded(event.nativeEvent.url)) view.current?.injectJavaScript(script);
      }}
      onNavigationStateChange={(state) => run.pageAt(state.url)}
      onError={(event) => {
        const reason = loadFailure(event.nativeEvent.code, event.nativeEvent.domain);
        logFailure(reason);
        run.pageFailed(reason);
      }}
      onHttpError={(event) => {
        const reason = httpFailure(event.nativeEvent.statusCode);
        logFailure(reason);
        run.pageFailed(reason);
      }}
      onShouldStartLoadWithRequest={(request) => {
        const go = navigation(page, request.url, request.isTopFrame);

        if (go === 'browser') openOutside(request.url);

        return go === 'load';
      }}
      onOpenWindow={(event) => openOutside(event.nativeEvent.targetUrl)}
      mediaCapturePermissionGrantType={grant.grantType}
      mediaCaptureHost={grant.host}
    />
  );
}

const styles = StyleSheet.create({
  page: { flex: 1 },
});
