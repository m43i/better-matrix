// An attachment's file leaving the app: fetched in full (FetchMedia, Original), copied under its
// own name, then handed to the system share sheet, or saved into a folder the user picks
// (Android; on iOS the share sheet's "Save to Files" and "Save Image" do that).

import type { AccountId, Attachment } from '@better-matrix/matrix-native/types';
import * as Clipboard from 'expo-clipboard';
import { Directory, File, Paths } from 'expo-file-system';
import * as Sharing from 'expo-sharing';
import { Platform } from 'react-native';

import { t } from '../../i18n';
import { fetchOriginal } from '../../state/actions/timeline';
import { addNotice } from '../../state/reducer';
import type { Store } from '../../state/store';

/** A name safe for a file system, keeping the attachment's own as far as possible. */
function safeName(name: string): string {
  const cleaned = [...name]
    .map((char) => (char.charCodeAt(0) < 32 || '/\\:*?"<>|'.includes(char) ? '_' : char))
    .join('')
    .trim();

  return cleaned === '' || cleaned === '.' || cleaned === '..' ? 'file' : cleaned;
}

const MIME = new Map(
  Object.entries({
    png: 'image/png',
    jpg: 'image/jpeg',
    jpeg: 'image/jpeg',
    gif: 'image/gif',
    webp: 'image/webp',
    heic: 'image/heic',
    mp4: 'video/mp4',
    mov: 'video/quicktime',
    webm: 'video/webm',
    ogg: 'audio/ogg',
    oga: 'audio/ogg',
    opus: 'audio/ogg',
    mp3: 'audio/mpeg',
    m4a: 'audio/mp4',
    pdf: 'application/pdf',
    txt: 'text/plain',
    zip: 'application/zip',
  }),
);

function mimeOf(name: string): string {
  const extension = name.split('.').pop()?.toLowerCase() ?? '';

  return MIME.get(extension) ?? 'application/octet-stream';
}

/** The attachment's file under its own name, in the cache. */
async function namedFile(store: Store, account: AccountId, attachment: Attachment): Promise<File> {
  if (!attachment.media) throw new Error('no file');
  const uri = await fetchOriginal(store, account, attachment.media);
  const folder = new Directory(Paths.cache, 'shared');
  folder.create({ intermediates: true, idempotent: true });
  const named = new File(folder, safeName(attachment.name));

  if (named.exists) named.delete();
  await new File(uri).copy(named);

  return named;
}

function cancelled(cause: unknown): boolean {
  return /cancel/i.test(String(cause));
}

/** Copies the original image, never the timeline's thumbnail or the file's name. */
export async function copyAttachmentImage(
  store: Store,
  account: AccountId,
  attachment: Attachment,
): Promise<boolean> {
  if (attachment.kind !== 'Image' || !attachment.media) return false;

  try {
    const uri = await fetchOriginal(store, account, attachment.media);
    await Clipboard.setImageAsync(await new File(uri).base64());

    return true;
  } catch (error) {
    store.update((s) => addNotice(s, t('notice.copy_image', { error: String(error) })));

    return false;
  }
}

/** Quick Look on iOS, or the system's ACTION_VIEW handler on Android. */
export async function openAttachment(
  store: Store,
  account: AccountId,
  attachment: Attachment,
): Promise<boolean> {
  try {
    const file = await namedFile(store, account, attachment);
    await file.preview({ mimeType: mimeOf(attachment.name), title: attachment.name });

    return true;
  } catch (error) {
    if (!cancelled(error))
      store.update((s) => addNotice(s, t('notice.open_attachment', { error: String(error) })));

    return false;
  }
}

/** Hands the attachment to the system share sheet. */
export async function shareAttachment(
  store: Store,
  account: AccountId,
  attachment: Attachment,
): Promise<boolean> {
  try {
    const file = await namedFile(store, account, attachment);
    await Sharing.shareAsync(file.uri, {
      mimeType: mimeOf(attachment.name),
      dialogTitle: attachment.name,
    });

    return true;
  } catch (error) {
    if (!cancelled(error))
      store.update((s) =>
        addNotice(s, t('mobile.conversation.share_failed', { error: String(error) })),
      );

    return false;
  }
}

/** Saves the attachment where the user picks (Android), or offers the share sheet's saving (iOS). */
export async function saveAttachment(
  store: Store,
  account: AccountId,
  attachment: Attachment,
): Promise<boolean> {
  if (Platform.OS !== 'android') {
    return shareAttachment(store, account, attachment);
  }

  try {
    const folder = await Directory.pickDirectoryAsync();
    const file = await namedFile(store, account, attachment);
    const saved = folder.createFile(safeName(attachment.name), mimeOf(attachment.name));
    await saved.write(await file.bytes());
    store.update((s) => addNotice(s, t('mobile.conversation.saved', { name: attachment.name })));

    return true;
  } catch (error) {
    if (!cancelled(error))
      store.update((s) => addNotice(s, t('notice.save', { error: String(error) })));

    return false;
  }
}
