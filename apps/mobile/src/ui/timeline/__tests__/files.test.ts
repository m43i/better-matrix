import { detachedEngine } from '@better-matrix/matrix-native/client';
import type { Attachment } from '@better-matrix/matrix-native/types';

import { Store } from '../../../state/store';
import { ADA } from '../../../test/fixtures';
import { copyAttachmentImage, openAttachment } from '../files';

const image: Attachment = {
  kind: 'Image',
  name: 'photo.png',
  media: 'original',
  thumbnail: 'thumbnail',
  dimensions: null,
  audio: null,
  sticker: false,
  size: null,
};

it('should fetch the original image and allow retry when copying fails', async () => {
  const { engine, sent } = detachedEngine();
  const store = new Store(engine);
  const copied = copyAttachmentImage(store, ADA, image);
  expect(sent[0]?.command).toEqual({
    type: 'FetchMedia',
    data: { account: ADA, media: 'original', size: { type: 'Original' } },
  });
  engine.emit({
    type: 'Finished',
    data: { request: sent[0]!.request, result: { Err: 'offline' } },
  });
  await expect(copied).resolves.toBe(false);
  expect(store.getState().notices.at(-1)?.message).toContain('offline');

  const retried = copyAttachmentImage(store, ADA, image);
  expect(sent).toHaveLength(2);
  engine.emit({
    type: 'Finished',
    data: { request: sent[1]!.request, result: { Err: 'offline' } },
  });
  await expect(retried).resolves.toBe(false);
});

it('should keep the file action unsuccessful when its original cannot be opened', async () => {
  const { engine, sent } = detachedEngine();
  const store = new Store(engine);
  const opened = openAttachment(store, ADA, image);
  engine.emit({
    type: 'Finished',
    data: { request: sent[0]!.request, result: { Err: 'missing file' } },
  });
  await expect(opened).resolves.toBe(false);
  expect(store.getState().notices.at(-1)?.message).toContain('missing file');
});

it('should not attempt to copy other attachments as images', async () => {
  const { engine, sent } = detachedEngine();
  await expect(
    copyAttachmentImage(new Store(engine), ADA, { ...image, kind: 'File' }),
  ).resolves.toBe(false);
  expect(sent).toEqual([]);
});
