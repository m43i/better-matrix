// The preview's file-only action sheet. Dismissing it returns to the attachment preview.

import { router, useLocalSearchParams } from 'expo-router';
import { useMemo } from 'react';
import { View } from 'react-native';

import { t } from '../i18n';
import { useStore } from '../state/react';
import { previewMessage } from '../state/selectors';
import { roomKey, threadKey } from '../state/timeline';
import { EmptyState } from '../ui/components';
import { usePalette } from '../ui/theme';
import { AttachmentActions } from '../ui/timeline/AttachmentActions';

export default function AttachmentActionsScreen() {
  const { account, roomId, eventId, thread } = useLocalSearchParams<{
    account: string;
    roomId: string;
    eventId: string;
    thread?: string;
  }>();

  const key = useMemo(
    () => (thread ? threadKey(roomId, thread) : roomKey(roomId)),
    [roomId, thread],
  );

  // Found as the preview found it: in a thread, or rebuilt from Room Info's files.
  const message = useStore((s) => previewMessage(s, account, key, eventId));
  const attachment = message?.content.type === 'Attachment' ? message.content.data : null;
  const palette = usePalette();

  return (
    <View
      testID="attachment-actions"
      style={{ padding: 16, paddingBottom: 32, backgroundColor: palette.bg }}
    >
      {message && attachment?.media ? (
        <AttachmentActions
          account={account}
          roomId={roomId}
          message={message}
          attachment={attachment}
          onDone={() => router.back()}
          offersShow
        />
      ) : (
        <EmptyState icon="info" title={t('mobile.conversation.message_missing')} />
      )}
    </View>
  );
}
