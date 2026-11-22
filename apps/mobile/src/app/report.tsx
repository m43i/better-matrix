// Reporting a message, a room or a person to the homeserver's admins (modal, ADR 0011): who sees
// it, an optional reason, and for a message or a person "Also ignore them". Report waits for the
// engine; success closes with a notice, a failure stays to try again.

import { useLocalSearchParams, useRouter } from 'expo-router';
import { useState } from 'react';
import { StyleSheet, Text } from 'react-native';

import { t } from '../i18n';
import { reportTarget } from '../routes';
import { afterReport, report, reportedPerson } from '../state/actions/safety';
import { useAppStore } from '../state/react';
import { font } from '../theme/palette';
import { ListRow, ListSection, TextField } from '../ui/components';
import { RoomForm } from '../ui/people/RoomForm';
import { usePalette } from '../ui/theme';
import { useShown } from '../ui/useShown';

export default function ReportScreen() {
  const params = useLocalSearchParams<{
    account: string;
    name: string;
    about?: string;
    roomId?: string;
    eventId?: string;
    userId?: string;
  }>();

  const store = useAppStore();
  const router = useRouter();
  const palette = usePalette();
  const [reason, setReason] = useState('');
  const [ignore, setIgnore] = useState(false);
  const [busy, setBusy] = useState(false);
  // Cancel stays available while it sends: an answer that comes after the user left moves nothing.
  const shown = useShown();
  const target = reportTarget(params);

  if (!target) return null;
  const { account, name } = params;
  const person = reportedPerson(target, params.userId ?? null);

  const title =
    target.type === 'Message'
      ? t('report.message_title')
      : target.type === 'Room'
        ? t('report.room_action')
        : t('report.person_title', { name });

  const submit = () => {
    if (busy) return;
    setBusy(true);
    report(store, account, target, reason, ignore ? person : null).then(
      () => {
        if (afterReport(true, shown.current) === 'close') router.back();
      },
      () => setBusy(false),
    );
  };

  return (
    <RoomForm
      testID="report"
      title={title}
      confirm={t('report.send')}
      valid
      busy={busy}
      onSubmit={submit}
    >
      <Text style={[font.body, styles.note, { color: palette.textMuted }]}>
        {target.type === 'Message'
          ? t('mobile.safety.report_message_body')
          : t('mobile.safety.report_body')}
      </Text>
      <TextField
        testID="report-reason"
        label={t('mobile.safety.reason')}
        placeholder={t('mobile.safety.reason_placeholder')}
        value={reason}
        onChangeText={setReason}
        multiline
        maxLength={1000}
        style={styles.reason}
      />
      {person !== null && person !== account ? (
        <ListSection>
          <ListRow
            testID="report-ignore"
            icon="block"
            title={t('report.also_ignore', { name })}
            toggle={{ value: ignore, onChange: setIgnore }}
          />
        </ListSection>
      ) : null}
    </RoomForm>
  );
}

const styles = StyleSheet.create({
  note: { paddingHorizontal: 4 },
  reason: { minHeight: 96, textAlignVertical: 'top' },
});
