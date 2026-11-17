// File actions shared by a message's action sheet and its full-screen attachment preview.

import type { AccountId, Attachment } from '@better-matrix/matrix-native/types';
import { useEffect, useRef, useState } from 'react';

import { t, type Key } from '../../i18n';
import { attachmentActions, type AttachmentAction } from '../../state/messages';
import { useAppStore } from '../../state/react';
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
  attachment,
  onDone,
}: {
  account: AccountId;
  attachment: Attachment;
  onDone: () => void;
}) {
  const store = useAppStore();
  const [busy, setBusy] = useState<AttachmentAction | null>(null);
  const mounted = useRef(false);
  useEffect(() => {
    mounted.current = true;

    return () => {
      mounted.current = false;
    };
  }, []);

  const run = (action: AttachmentAction) => {
    if (busy !== null) return;
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

  return attachmentActions(attachment).map((action) => (
    <ListRow
      key={action}
      icon={actions[action].icon}
      title={t(actions[action].label)}
      chevron={false}
      value={busy === action ? '…' : undefined}
      testID={`attachment-actions-${action}`}
      onPress={busy === null ? () => run(action) : undefined}
    />
  ));
}
