// File actions shared by a message's action sheet and its full-screen attachment preview.

import type { AccountId, Attachment, Message } from '@better-matrix/matrix-native/types';
import { useEffect, useRef, useState } from 'react';

import { t, type Key } from '../../i18n';
import { type AttachmentAction, attachmentRows } from '../../state/messages';
import { revealMedia } from '../../state/actions/safety';
import { useAppStore, useStore } from '../../state/react';
import { previewHidden } from '../../state/selectors';
import { ListRow } from '../components';
import type { IconName } from '../icons';
import { copyAttachmentImage, openAttachment, saveAttachment, shareAttachment } from './files';

const actions = {
  copyImage: { icon: 'copy', label: 'attachment.copy_image', run: copyAttachmentImage },
  open: { icon: 'external', label: 'attachment.open', run: openAttachment },
  share: { icon: 'share', label: 'mobile.conversation.share', run: shareAttachment },
  save: { icon: 'save', label: 'preview.download', run: saveAttachment },
} satisfies Record<AttachmentAction, { icon: IconName; label: Key; run: typeof shareAttachment }>;

export function AttachmentActions({
  account,
  roomId,
  message,
  attachment,
  onDone,
  offersShow = false,
}: {
  account: AccountId;
  /** The room and message it was sent in, which decide whether its media is held (ADR 0011). */
  roomId: string;
  message: Message;
  attachment: Attachment;
  onDone: () => void;
  /** Offer Show while the media is held back, as the sheet has nothing else (`attachmentRows`). */
  offersShow?: boolean;
}) {
  const store = useAppStore();
  const held = useStore((s) => previewHidden(s, account, roomId, message));
  const [busy, setBusy] = useState<AttachmentAction | null>(null);
  const mounted = useRef(false);
  useEffect(() => {
    mounted.current = true;

    return () => {
      mounted.current = false;
    };
  }, []);

  const run = (action: AttachmentAction) => {
    // Checked again as it runs: nothing held back is fetched, whatever the sheet showed.
    if (busy !== null || previewHidden(store.getState(), account, roomId, message)) return;
    setBusy(action);
    void actions[action]
      .run(store, account, attachment)
      .then((done) => {
        if (done && mounted.current) onDone();
      })
      .finally(() => {
        if (mounted.current) setBusy(null);
      });
  };

  return attachmentRows(attachment, held, offersShow).map((action) =>
    action === 'show' ? (
      <ListRow
        key={action}
        icon="eye"
        title={t('safety.show_media')}
        chevron={false}
        testID="attachment-actions-show"
        onPress={() => revealMedia(store, account, message.event_id)}
      />
    ) : (
      <ListRow
        key={action}
        icon={actions[action].icon}
        title={t(actions[action].label)}
        chevron={false}
        value={busy === action ? '…' : undefined}
        testID={`attachment-actions-${action}`}
        onPress={busy === null ? () => run(action) : undefined}
      />
    ),
  );
}
