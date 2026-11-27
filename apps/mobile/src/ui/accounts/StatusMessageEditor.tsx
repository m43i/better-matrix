// Write the status message shared with Matrix presence and choose when this device clears it.

import { useRouter } from 'expo-router';
import { useState } from 'react';
import { StyleSheet, Text, TextInput, View } from 'react-native';
import { KeyboardAwareScrollView } from 'react-native-keyboard-controller';

import { t } from '../../i18n';
import { setStatusMessage } from '../../state/actions/accounts';
import { useAppStore } from '../../state/react';
import { sendTimeText } from '../../state/schedule';
import {
  clearStatusAt,
  STATUS_DURATIONS,
  type StatusExpiry,
  statusMessageDeadline,
} from '../../state/statusMessage';
import type { AccountData } from '../../state/types';
import { font } from '../../theme/palette';
import { Button, IconButton, ListRow, ListSection } from '../components';
import { DateTimeFields } from '../DateTimeFields';
import { useNow } from '../shell/useNow';
import { usePalette } from '../theme';
import { useShown } from '../useShown';

const suggestions = [
  ['vacation', '🌴', 'status_message.vacation'],
  ['meeting', '📅', 'status_message.meeting'],
  ['sick', '🤒', 'status_message.sick'],
  ['remote', '🏠', 'status_message.remote'],
] as const;

export function StatusMessageEditor({ account }: { account: AccountData }) {
  const store = useAppStore();
  const router = useRouter();
  const palette = usePalette();
  const shown = useShown();
  const now = useNow();
  const [message, setMessage] = useState(account.statusMessage ?? '');

  const [expiry, setExpiry] = useState<StatusExpiry>(() => {
    const at = statusMessageDeadline(account, now);

    return at === null ? STATUS_DURATIONS[0] : { at };
  });

  const [picker, setPicker] = useState<'closed' | 'choices' | 'custom'>('closed');
  const [customAt, setCustomAt] = useState(now + 60 * 60_000);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const timed = clearStatusAt(expiry, now);

  const expiryLabel =
    'at' in expiry
      ? timed === null
        ? t('status_message.expiry_never')
        : sendTimeText(timed, now)
      : t(expiry.label);

  const togglePicker = () => {
    if (picker === 'custom' && customAt > Date.now()) setExpiry({ at: customAt });

    setPicker(picker === 'closed' ? 'choices' : 'closed');
  };

  const save = async (text: string) => {
    if (busy) return;
    const savingAt = Date.now();
    const at = picker === 'custom' ? customAt : clearStatusAt(expiry, savingAt);

    if (text.trim() && at !== null && at <= savingAt) {
      setError(t('status_message.expiry_time_invalid'));

      return;
    }

    setBusy(true);
    setError(null);

    try {
      await setStatusMessage(store, account.summary.id, text, at);

      if (shown.current) router.back();
    } catch (failure) {
      if (shown.current) {
        setError(
          t('notice.status_message', {
            error: failure instanceof Error ? failure.message : String(failure),
          }),
        );
      }
    } finally {
      if (shown.current) setBusy(false);
    }
  };

  return (
    <KeyboardAwareScrollView
      testID="status-message-editor"
      style={{ backgroundColor: palette.bg }}
      contentContainerStyle={styles.content}
      contentInsetAdjustmentBehavior="automatic"
      keyboardShouldPersistTaps="handled"
      bottomOffset={24}
    >
      <View
        style={[styles.field, { backgroundColor: palette.surface, borderColor: palette.divider }]}
      >
        <TextInput
          testID="status-message-field"
          accessibilityLabel={t('status_message.set')}
          placeholder={t('status_message.placeholder')}
          placeholderTextColor={palette.textMuted}
          selectionColor={palette.accent}
          style={[font.body, styles.input, { color: palette.text }]}
          value={message}
          editable={!busy}
          onChangeText={setMessage}
          onSubmitEditing={() => void save(message)}
          returnKeyType="done"
        />
        <IconButton
          testID="status-expiry"
          icon="calendar"
          label={`${t('status_message.expiry_label')} · ${expiryLabel}`}
          expanded={picker !== 'closed'}
          color={picker !== 'closed' || timed !== null ? palette.accent : palette.textMuted}
          size={44}
          onPress={togglePicker}
        />
      </View>
      {picker === 'closed' && timed !== null ? (
        <Text style={[font.meta, styles.expirySummary, { color: palette.textMuted }]}>
          {t('status_message.expires_at', { time: sendTimeText(timed, now) })}
        </Text>
      ) : null}
      {picker === 'choices' ? (
        <ListSection title={t('status_message.expiry_label')}>
          {STATUS_DURATIONS.map((duration) => (
            <ListRow
              key={duration.id}
              testID={`status-expiry-${duration.id}`}
              title={t(duration.label)}
              selected={
                'at' in expiry
                  ? timed === null && duration.id === 'never'
                  : expiry.id === duration.id
              }
              chevron={false}
              onPress={() => {
                setExpiry(duration);
                setPicker('closed');
              }}
            />
          ))}
          <ListRow
            testID="status-expiry-custom"
            icon="calendar"
            title={t('status_message.expiry_custom')}
            selected={'at' in expiry && timed !== null}
            onPress={() => {
              setCustomAt('at' in expiry && timed !== null ? timed : Date.now() + 60 * 60_000);
              setPicker('custom');
            }}
          />
        </ListSection>
      ) : null}
      {picker === 'custom' ? (
        <>
          <ListSection>
            <ListRow
              testID="status-expiry-back"
              icon="back"
              title={t('status_message.expiry_back')}
              chevron={false}
              onPress={() => setPicker('choices')}
            />
          </ListSection>
          <DateTimeFields
            testID="status-expiry"
            value={customAt}
            onChange={setCustomAt}
            now={now}
            dateLabel={t('mobile.composer.schedule_date')}
            timeLabel={t('status_message.expiry_time')}
          />
        </>
      ) : null}
      {picker === 'closed' ? (
        <ListSection title={t('status_message.suggestions')}>
          {suggestions.map(([id, emoji, label]) => (
            <ListRow
              key={id}
              testID={`status-suggestion-${id}`}
              title={t(label)}
              leading={<Text style={[font.body, { color: palette.text }]}>{emoji}</Text>}
              chevron={false}
              onPress={() => setMessage(`${emoji} ${t(label)}`)}
            />
          ))}
        </ListSection>
      ) : null}
      <View style={styles.actions}>
        {error ? (
          <Text accessibilityRole="alert" style={[font.body, { color: palette.danger }]}>
            {error}
          </Text>
        ) : null}
        <Button
          title={t('status_message.save')}
          testID="status-message-save"
          busy={busy}
          onPress={() => void save(message)}
        />
        {account.statusMessage !== null ? (
          <Button
            title={t('status_message.clear')}
            kind="plain"
            disabled={busy}
            onPress={() => void save('')}
          />
        ) : null}
      </View>
    </KeyboardAwareScrollView>
  );
}

const styles = StyleSheet.create({
  content: { paddingBottom: 24 },
  field: {
    flexDirection: 'row',
    alignItems: 'center',
    marginHorizontal: 16,
    marginTop: 16,
    borderWidth: StyleSheet.hairlineWidth,
    borderRadius: 12,
    borderCurve: 'continuous',
  },
  input: { flex: 1, minHeight: 48, paddingHorizontal: 12 },
  expirySummary: { paddingHorizontal: 16, paddingTop: 8 },
  actions: { gap: 12, paddingHorizontal: 16, paddingTop: 16 },
});
