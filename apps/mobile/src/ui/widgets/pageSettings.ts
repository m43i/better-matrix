// The widget web view's settings every run shares, meetings too (`WidgetPageView`), apart from
// the component so tests read them without the native view.

import type { WebViewProps } from 'react-native-webview';

/** The web view's settings every run shares, meetings too. */
export const PAGE_SETTINGS = {
  // Every scheme reaches the navigation policy: otherwise the web view hands unlisted ones to
  // the system before asking.
  originWhitelist: ['*'],
  // No widget reads where the user is: Android's web view would grant it whenever the app may.
  geolocationEnabled: false,
  allowsInlineMediaPlayback: true,
  mediaPlaybackRequiresUserAction: false,
  allowsFullscreenVideo: true,
  domStorageEnabled: true,
  javaScriptEnabled: true,
  injectedJavaScriptBeforeContentLoadedForMainFrameOnly: true,
} satisfies WebViewProps;
