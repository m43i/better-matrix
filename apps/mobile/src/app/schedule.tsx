// Scheduling a message (sheet, from a long press on Send), as Slack's schedule menu: in 30
// minutes, in an hour, tomorrow at 9, next Monday at 9, each with when that is, or a date and
// time picked. Choosing one hands the composer's text to the server (MSC4140) and closes.

import { useLocalSearchParams, useRouter } from 'expo-router';
import { useState } from 'react';
import { Alert, ScrollView, StyleSheet, Text, View } from 'react-native';

import { type Key, t } from '../i18n';
import { keyOf, scheduleComposer } from '../state/actions/composer';
import { useAppStore } from '../state/react';
import {
  delayUntil,
  PRESETS,
  presetTime,
  type SchedulePreset,
  sendTimeText,
} from '../state/schedule';
import { font } from '../theme/palette';
import { Button, ListRow, ListSection } from '../ui/components';
import { DateTimeFields } from '../ui/DateTimeFields';
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

  const scheduleAt = (at: number) => {
    const delay = delayUntil(at, Date.now());

    if (delay === null) {
      Alert.alert(t('mobile.composer.schedule'), t('mobile.composer.schedule_range'));

      return;
    }

    // Refused, the draft stays and a notice in the conversation says why.
    scheduleComposer(store, account, keyOf(roomId, thread), delay);
    router.back();
  };

  const startCustom = () => setCustom(presetTime('inHour', Date.now()));

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
          <DateTimeFields
            testID="schedule"
            value={custom}
            onChange={setCustom}
            dateLabel={t('mobile.composer.schedule_date')}
            timeLabel={t('mobile.composer.schedule_time')}
            now={now}
            latest={now + MAX_DAYS * 86_400_000}
          />
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
});
