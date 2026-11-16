// Which demo scene a link asks for: `bettermatrix://demo/<scene>`. Dev builds only.

import { isScene, type SceneName } from './scenes';

/** The scene `url` names, or `null` when it isn't a demo link (or names no scene). */
export function demoScene(url: string | null | undefined): SceneName | null {
  if (!url) return null;
  const match = /^(?:[a-z][a-z0-9+.-]*:\/\/|\/)demo\/([a-z0-9-]+)\/?(?:[?#].*)?$/i.exec(url);
  const name = match?.[1];
  return name !== undefined && isScene(name) ? name : null;
}
