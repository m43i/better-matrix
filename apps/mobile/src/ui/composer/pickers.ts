// The platform's own pickers for attachments: the photo library (several at once, in the order
// picked), the camera, and files. Each picked file becomes a `PendingAttachment` with a plain
// path; when the picker's copy has a made-up name, it is copied once more under the name the
// user knows, so it arrives under that name (the engine names an upload after its path).

import * as DocumentPicker from 'expo-document-picker';
import { Directory, File, Paths } from 'expo-file-system';
import * as ImagePicker from 'expo-image-picker';

import type { PendingAttachment } from '../../state/composer';
import { attachmentKind, fileName, pathFromUri, uploadName } from '../../state/outgoing';

/** Why nothing came back: the user said no to the camera. */
export const DENIED = 'denied';

type Kind = PendingAttachment['kind'];

/** A unique folder in the cache for one copy. */
let copies = 0;

/**
 * `uri` as an attachment named `given` (with the extension on disk), previewing as its media type
 * or name says, else as `fallback`.
 */
async function attachment(
  uri: string,
  given: string | null | undefined,
  mime: string | null | undefined,
  fallback: Kind = 'file',
): Promise<PendingAttachment> {
  const original = pathFromUri(uri);
  const name = uploadName(original, given);
  const kind = attachmentKind(mime, name, fallback);

  if (name === fileName(original)) return { path: original, name, kind, uri };

  try {
    const folder = new Directory(Paths.cache, 'uploads', `${Date.now()}-${copies++}`);
    folder.create({ intermediates: true, idempotent: true });
    const copy = new File(folder, name);
    await new File(uri).copy(copy);

    return { path: pathFromUri(copy.uri), name, kind, uri: copy.uri };
  } catch {
    // The picker's own copy still sends, under its own name.
    return { path: original, name: fileName(original), kind, uri };
  }
}

function fromAssets(assets: ImagePicker.ImagePickerAsset[]): Promise<PendingAttachment[]> {
  return Promise.all(
    assets.map((asset) =>
      attachment(
        asset.uri,
        asset.fileName,
        asset.mimeType,
        asset.type === 'video' ? 'video' : 'image',
      ),
    ),
  );
}

/** Photos and videos from the library, several at once. */
export async function pickFromLibrary(): Promise<PendingAttachment[]> {
  const result = await ImagePicker.launchImageLibraryAsync({
    mediaTypes: ['images', 'videos'],
    allowsMultipleSelection: true,
    orderedSelection: true,
    selectionLimit: 0,
    quality: 1,
  });

  return result.canceled ? [] : fromAssets(result.assets);
}

/** Pictures from the library, several at once, kept as they are (stickers keep their size). */
export async function pickPictures(): Promise<PendingAttachment[]> {
  const result = await ImagePicker.launchImageLibraryAsync({
    mediaTypes: ['images'],
    allowsMultipleSelection: true,
    orderedSelection: true,
    selectionLimit: 0,
    quality: 1,
  });

  return result.canceled ? [] : fromAssets(result.assets);
}

/** A photo or video taken now; `DENIED` when the camera isn't allowed. */
export async function takeWithCamera(): Promise<PendingAttachment[] | typeof DENIED> {
  const permission = await ImagePicker.requestCameraPermissionsAsync();

  if (!permission.granted) return DENIED;

  const result = await ImagePicker.launchCameraAsync({
    mediaTypes: ['images', 'videos'],
    quality: 1,
  });

  return result.canceled ? [] : fromAssets(result.assets);
}

/** Any files, several at once. */
export async function pickFiles(): Promise<PendingAttachment[]> {
  const result = await DocumentPicker.getDocumentAsync({
    multiple: true,
    copyToCacheDirectory: true,
  });

  if (result.canceled) return [];

  return Promise.all(
    result.assets.map((asset) => attachment(asset.uri, asset.name, asset.mimeType)),
  );
}
