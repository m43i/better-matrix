// Copies libwebrtc's Java half (libwebrtc.jar, from the prebuilt libwebrtc the Rust build
// downloaded) next to the engine's libraries, as `android/libs/libwebrtc.jar`. libwebrtc's native
// code calls into these classes (`livekit.org.webrtc.*`: the audio device module, the codec
// factories, the camera capturer), so the app has to ship them; the call-media module
// (apps/mobile/modules/call-media) compiles against this copy. Runs after `ubrn build android`.

import { copyFileSync, mkdirSync, readdirSync, statSync } from 'node:fs';
import { dirname, join } from 'node:path';

const root = join(import.meta.dir, '..', '..', '..');
const target = join(root, 'target');
const out = join(import.meta.dir, '..', 'android', 'libs', 'libwebrtc.jar');

/** Every libwebrtc.jar the Android builds unpacked under `dir`. */
function find(dir: string, depth = 0): string[] {
  if (depth > 12) return [];
  let entries: string[];
  try {
    entries = readdirSync(dir);
  } catch {
    return [];
  }
  return entries.flatMap((name) => {
    const path = join(dir, name);
    if (name === 'libwebrtc.jar' && dir.includes('android')) return [path];
    try {
      return statSync(path).isDirectory() ? find(path, depth + 1) : [];
    } catch {
      return [];
    }
  });
}

const jars = find(target).sort((a, b) => statSync(b).mtimeMs - statSync(a).mtimeMs);
const newest = jars[0];
if (!newest) {
  console.error('webrtc-jar: no libwebrtc.jar under target/; build the engine for Android first.');
  process.exit(1);
}
mkdirSync(dirname(out), { recursive: true });
copyFileSync(newest, out);
console.log(`webrtc-jar: ${newest} -> ${out}`);
