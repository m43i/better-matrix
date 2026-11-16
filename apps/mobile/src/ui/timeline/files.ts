// An attachment's file leaving the app: fetched in full (FetchMedia, Original), copied under its
// own name, then handed to the system share sheet, or saved into a folder the user picks
// (Android; on iOS the share sheet's "Save to Files" and "Save Image" do that).

import type { AccountId, Attachment } from '@better-matrix/matrix-native/types';
import { Directory, File, Paths } from 'expo-file-system';
import * as Sharing from 'expo-sharing';
import { Platform } from 'react-native';

import { t } from '../../i18n';
import { fetchOriginal } from '../../state/actions/timeline';
import { addNotice } from '../../state/reducer';
import type { Store } from '../../state/store';

/** A name safe for a file system, keeping the attachment's own as far as possible. */
function safeName(name: string): string {
  const cleaned = name.replace(/[/\\:*?"<>|\u0000-\u001f]/g, '_').trim();
  return cleaned === '' || cleaned === '.' || cleaned === '..' ? 'file' : cleaned;
}

const MIME: Record<string, string> = {
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
};

function mimeOf(name: string): string {
  const extension = name.split('.').pop()?.toLowerCase() ?? '';
  return MIME[extension] ?? 'application/octet-stream';
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

function cancelled(error: unknown): boolean {
  return /cancel/i.test(String(error));
}

/** Hands the attachment to the system share sheet. */
export async function shareAttachment(store: Store, account: AccountId, attachment: Attachment): Promise<void> {
  try {
    const file = await namedFile(store, account, attachment);
    await Sharing.shareAsync(file.uri, { mimeType: mimeOf(attachment.name), dialogTitle: attachment.name });
  } catch (error) {
    if (!cancelled(error)) store.update((s) => addNotice(s, t('mobile.conversation.share_failed', { error: String(error) })));
  }
}

/** Saves the attachment where the user picks (Android), or offers the share sheet's saving (iOS). */
export async function saveAttachment(store: Store, account: AccountId, attachment: Attachment): Promise<void> {
  if (Platform.OS !== 'android') {
    await shareAttachment(store, account, attachment);
    return;
  }
  try {
    const folder = await Directory.pickDirectoryAsync();
    const file = await namedFile(store, account, attachment);
    const saved = folder.createFile(safeName(attachment.name), mimeOf(attachment.name));
    await saved.write(await file.bytes());
    store.update((s) => addNotice(s, t('mobile.conversation.saved', { name: attachment.name })));
  } catch (error) {
    if (!cancelled(error)) store.update((s) => addNotice(s, t('notice.save', { error: String(error) })));
  }
}
