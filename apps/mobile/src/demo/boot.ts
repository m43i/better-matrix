// Starts the app on a demo scene: a store on the demo engine, seeded through `apply`.

import { setLanguage } from '../i18n';
import { mxcMedia } from '../state/messages';
import { DEFAULT_SETTINGS } from '../state/settings';
import { Store } from '../state/store';
import { demoEngine, type MediaResolver } from './engine';
import { DEMO_MEDIA, DEMO_PACK_IMAGES } from './fixtures';
import { type Scene, sceneEvents, scenes, type SceneName } from './scenes';

/** The app's own images, standing in for media on a server. */
const ASSETS = new Map<string, number>([
  [DEMO_MEDIA.screenshot, require('../../assets/images/icon.png')],
  [DEMO_MEDIA.release, require('../../assets/images/android-icon-background.png')],
  [DEMO_MEDIA.voice, require('../../assets/sounds/ring.wav')],
  [mxcMedia(DEMO_PACK_IMAGES.party), require('../../assets/demo/emote-party.png')],
  [mxcMedia(DEMO_PACK_IMAGES.ok), require('../../assets/demo/emote-ok.png')],
  [mxcMedia(DEMO_PACK_IMAGES.wave), require('../../assets/demo/sticker-wave.png')],
  [mxcMedia(DEMO_PACK_IMAGES.ship), require('../../assets/demo/sticker-ship.png')],
  [mxcMedia(DEMO_PACK_IMAGES.coffee), require('../../assets/demo/sticker-coffee.png')],
]);

/** One street map stands in for every place (the engine draws real ones from OpenStreetMap). */
const MAP: number = require('../../assets/demo/map.png');

async function resolveMedia(media: string): Promise<string | null> {
  const module = media.startsWith(DEMO_MEDIA.map) ? MAP : ASSETS.get(media);

  if (module === undefined) return null;
  const { Asset } = await import('expo-asset');
  const asset = await Asset.fromModule(module).downloadAsync();

  return asset.localUri ? decodeURI(asset.localUri.replace(/^file:\/\//, '')) : null;
}

export function bootDemo(name: SceneName, media: MediaResolver = resolveMedia): Store {
  const scene: Scene = scenes[name];
  const settings = { ...DEFAULT_SETTINGS, ...scene.settings };
  setLanguage(settings.language);
  const engine = demoEngine(media);
  const store = new Store(engine, settings);

  for (const event of sceneEvents(scene)) engine.emit(event);
  scene.setup?.(store);

  return store;
}
