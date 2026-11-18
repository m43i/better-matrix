// Media for views: fetched once when a view first needs it, shown once its file is there.

import type { AccountId, MediaRef, MediaSize } from '@better-matrix/matrix-native/types';
import { useEffect } from 'react';

import { fetchMedia } from '../state/actions/core';
import { useAppStore, useStore } from '../state/react';
import { mediaUri } from '../state/selectors';

/** The size avatars are fetched at: crisp at 3x for the largest avatar the app draws. */
export const AVATAR_SIZE: MediaSize = { type: 'Thumbnail', data: { width: 192, height: 192 } };

/** The size timelines fetch image previews at, as the desktop's `MediaSize::PREVIEW`. */
export const PREVIEW_SIZE: MediaSize = { type: 'Thumbnail', data: { width: 800, height: 600 } };

export const ORIGINAL: MediaSize = { type: 'Original' };

/**
 * When a view still waiting for its media asks again, after the first ask: a fetch that failed
 * (offline, or its file couldn't be written) is fetched again; one under way isn't.
 */
export const MEDIA_RETRY_MS = [2_000, 10_000, 30_000];

/** A `file://` URI for `media` at `size`, once fetched; fetches it the first time it's needed. */
export function useMedia(
  account: AccountId | null,
  media: MediaRef | null | undefined,
  size: MediaSize,
): string | undefined {
  const store = useAppStore();
  const uri = useStore((s) => (media ? mediaUri(s, media, size) : undefined));
  useEffect(() => {
    if (!account || !media || uri) return;
    fetchMedia(store, account, media, size);

    const timers = MEDIA_RETRY_MS.map((ms) =>
      setTimeout(() => fetchMedia(store, account, media, size), ms),
    );

    return () => {
      for (const timer of timers) clearTimeout(timer);
    };
    // `size` is a constant per call site; comparing its fields keeps the effect stable.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [store, account, media, uri, size.type === 'Thumbnail' ? size.data.width : 0]);

  return uri;
}
