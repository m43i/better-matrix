// Opens a demo scene's screens, one after another, once the navigator is ready.

import { router, useRootNavigationState } from 'expo-router';
import { useEffect } from 'react';

import { type Scene, type SceneName, scenes } from './scenes';

/** How long a screen gets to mount before the next one opens over it. */
const STEP_MS = 400;

export function DemoNavigation({ scene }: { scene: SceneName }) {
  const ready = useRootNavigationState()?.key !== undefined;
  useEffect(() => {
    if (!ready) return;
    const definition: Scene = scenes[scene];
    const steps = definition.routes ?? [];
    let cancelled = false;
    void (async () => {
      // Every scene starts from Home, whatever the last one left open.
      try {
        if (router.canDismiss()) router.dismissAll();
        router.replace('/');
      } catch {
        // The tabs are still mounting for the new scene: they start at Home anyway.
      }

      for (const href of steps) {
        await new Promise((resolve) => setTimeout(resolve, STEP_MS));

        if (cancelled) return;
        router.push(href);
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [scene, ready]);

  return null;
}
