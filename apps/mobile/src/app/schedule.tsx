// Scheduling a message (sheet, from a long press on Send), as Slack's schedule menu: in 30
// minutes, in an hour, tomorrow at 9, next Monday at 9, each with when that is, or a date and
// time picked. Choosing one hands the composer's text to the server (MSC4140) and closes.

import { DateTimePicker } from '@expo/ui/community/datetime-picker';
import { useLocalSearchParams, useRouter } from 'expo-router';
import { useState } from 'react';
import { Alert, Platform, ScrollView, StyleSheet, Text, View } from 'react-native';

import { type Key, t } from '../i18n';
import { keyOf, scheduleComposer } from '../state/actions/composer';
import { useAppStore } from '../state/react';
import {
  delayUntil,
  PRESETS,
  presetTime,
  type SchedulePreset,
  sendTimeText,
  withTimeOf,
} from '../state/schedule';
import { font } from '../theme/palette';
import { Button, ListRow, ListSection } from '../ui/components';
import { useNow } from '../ui/shell/useNow';
import { usePalette } from '../ui/theme';

const LABELS: Record<SchedulePreset, Key> = {
  in30: 'mobile.composer.schedule_in_30',
  inHour: 'mobile.composer.schedule_in_hour',
  tomorrow: 'mobile.composer.schedule_tomorrow',
  monday: 'mobile.composer.schedule_monday',
};

const MAX_DAYS = 30;

export default function ScheduleScreen() {
  const { account, roomId, thread } = useLocalSearchParams<{
    account: string;
    roomId: string;
    thread?: string;
  }>();

  const store = useAppStore();
  const router = useRouter();
  const palette = usePalette();
  const now = useNow();
  const [custom, setCustom] = useState<number | null>(null);
  /** The Android picker dialog showing, if any (iOS draws its pickers in place). */
  const [picking, setPicking] = useState<'date' | 'time' | null>(null);

  const scheduleAt = (at: number) => {
    const delay = delayUntil(at, Date.now());

    if (delay === null) {
      Alert.alert(t('mobile.composer.schedule'), t('mobile.composer.schedule_range'));

      return;
    }

    if (scheduleComposer(store, account, keyOf(roomId, thread), delay)) router.back();
  };

  const startCustom = () => setCustom(presetTime('inHour', Date.now()));

  const pick = (mode: 'date' | 'time', date: Date) => {
    setPicking(null);
    setCustom((before) => {
      const base = before ?? Date.now();

      return mode === 'date' ? withTimeOf(date.getTime(), base) : withTimeOf(base, date.getTime());
    });
  };

  const picker = (mode: 'date' | 'time', value: number) => (
    <DateTimePicker
      testID={`schedule-${mode}-picker`}
      value={new Date(value)}
      mode={mode}
      display={Platform.OS === 'ios' ? 'compact' : 'default'}
      minimumDate={mode === 'date' ? new Date(now) : undefined}
      maximumDate={mode === 'date' ? new Date(now + MAX_DAYS * 86_400_000) : undefined}
      accentColor={palette.accent}
      is24Hour
      onValueChange={(_, date) => pick(mode, date)}
      onDismiss={() => setPicking(null)}
    />
  );

  return (
    <ScrollView
      testID="schedule"
      style={{ backgroundColor: palette.bg }}
      contentContainerStyle={styles.content}
    >
      <Text style={[font.header, styles.title, { color: palette.text }]}>
        {t('mobile.composer.schedule')}
      </Text>
      <ListSection>
        {PRESETS.map((preset) => (
          <ListRow
            key={preset}
            icon="clock"
            title={t(LABELS[preset])}
            value={sendTimeText(presetTime(preset, now), now)}
            chevron={false}
            onPress={() => scheduleAt(presetTime(preset, Date.now()))}
            testID={`schedule-${preset}`}
          />
        ))}
        <ListRow
          icon="scheduleSend"
          title={t('mobile.composer.schedule_custom')}
          chevron={false}
          selected={custom !== null ? true : undefined}
          onPress={startCustom}
          testID="schedule-custom"
        />
      </ListSection>
      {custom !== null ? (
        <View style={styles.custom}>
          <ListSection>
            {Platform.OS === 'ios' ? (
              <>
                <View style={styles.inline}>
                  <Text style={[font.body, { color: palette.text }]}>
                    {t('mobile.composer.schedule_date')}
                  </Text>
                  {picker('date', custom)}
                </View>
                <View style={styles.inline}>
                  <Text style={[font.body, { color: palette.text }]}>
                    {t('mobile.composer.schedule_time')}
                  </Text>
                  {picker('time', custom)}
                </View>
              </>
            ) : (
              <>
                <ListRow
                  icon="calendar"
                  title={t('mobile.composer.schedule_date')}
                  value={sendTimeText(custom, now).split(' · ')[0]}
                  onPress={() => setPicking('date')}
                  testID="schedule-date"
                />
                <ListRow
                  icon="clock"
                  title={t('mobile.composer.schedule_time')}
                  value={sendTimeText(custom, now).split(' · ')[1]}
                  onPress={() => setPicking('time')}
                  testID="schedule-time"
                />
              </>
            )}
          </ListSection>
          {picking && Platform.OS !== 'ios' ? picker(picking, custom) : null}
          <View style={styles.confirm}>
            <Button
              title={t('mobile.composer.schedule_confirm')}
              icon="scheduleSend"
              onPress={() => scheduleAt(custom)}
              testID="schedule-confirm"
            />
          </View>
        </View>
      ) : null}
    </ScrollView>
  );
}

const styles = StyleSheet.create({
  content: { paddingBottom: 32 },
  title: { paddingHorizontal: 16, paddingTop: 20 },
  custom: { gap: 16 },
  confirm: { paddingHorizontal: 16 },
  inline: {
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'space-between',
    paddingHorizontal: 16,
    paddingVertical: 8,
  },
});
