// The main open-source software the mobile app is built from, with its license: the engine's
// Matrix and call stack (as the desktop's apps/desktop/assets/third-party.tsv lists them) and the
// React Native and Expo libraries the app runs on. Keep the versions in step with package.json
// and Cargo.lock when bumping.

export interface Component {
  name: string;
  version: string;
  license: string;
}

export const THIRD_PARTY: readonly Component[] = [
  { name: 'matrix-sdk', version: '0.19.1', license: 'Apache-2.0' },
  { name: 'matrix-sdk-crypto', version: '0.19.1', license: 'Apache-2.0' },
  { name: 'vodozemac', version: '0.11.2', license: 'Apache-2.0' },
  { name: 'ruma', version: '0.17.2', license: 'MIT' },
  { name: 'livekit', version: '0.11.0', license: 'Apache-2.0' },
  { name: 'libwebrtc', version: 'a545de4', license: 'BSD-3-Clause' },
  { name: 'tokio', version: '1.54.1', license: 'MIT' },
  { name: 'rusqlite', version: '0.40.2', license: 'MIT' },
  { name: 'serde', version: '1.0.229', license: 'MIT OR Apache-2.0' },
  { name: 'uniffi-bindgen-react-native', version: '0.31.0-7', license: 'MPL-2.0' },
  { name: 'React', version: '19.3.0', license: 'MIT' },
  { name: 'React Native', version: '0.88.1', license: 'MIT' },
  { name: 'Expo', version: '58.0.25', license: 'MIT' },
  { name: 'Expo Router', version: '58.0.31', license: 'MIT' },
  { name: 'react-native-screens', version: '4.30.1', license: 'MIT' },
  { name: 'react-native-reanimated', version: '4.7.1', license: 'MIT' },
  { name: 'react-native-worklets', version: '0.13.0', license: 'MIT' },
  { name: 'react-native-gesture-handler', version: '3.3.0', license: 'MIT' },
  { name: 'react-native-keyboard-controller', version: '1.22.7', license: 'MIT' },
  { name: 'react-native-safe-area-context', version: '5.11.1', license: 'MIT' },
  { name: 'FlashList', version: '2.3.2', license: 'MIT' },
  { name: 'htmlparser2', version: '12.0.0', license: 'MIT' },
  { name: 'Emojibase', version: '18.0.0', license: 'MIT' },
];
