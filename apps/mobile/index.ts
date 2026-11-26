// The app's entry: the push task is defined before anything else loads, so a push that starts
// the app's JavaScript in the background finds it (src/engine/pushTask.ts); then the router.

import './src/engine/pushTask';
import './src/engine/locationTask';
import 'expo-router/entry';
