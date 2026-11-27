// Two native selectors for a local date and time. iOS shows compact pickers in place.
// Android opens the platform's date or time dialog from the corresponding row.

import { DateTimePicker } from '@expo/ui/community/datetime-picker';
import { useState } from 'react';
import { Platform, StyleSheet, Text, View } from 'react-native';

import { sendTimeText, withTimeOf } from '../state/schedule';
import { font } from '../theme/palette';
import { ListRow, ListSection } from './components';
import { usePalette } from './theme';

type Field = 'date' | 'time';

export function DateTimeFields({
  value,
  onChange,
  now,
  latest,
  dateLabel,
  timeLabel,
  testID,
}: {
  value: number;
  onChange: (value: number) => void;
  now: number;
  latest?: number;
  dateLabel: string;
  timeLabel: string;
  testID: string;
}) {
  const palette = usePalette();
  const [picking, setPicking] = useState<Field | null>(null);
  const [dateText, timeText] = sendTimeText(value, now).split(' · ');

  const picker = (field: Field) => (
    <DateTimePicker
      testID={`${testID}-${field}-picker`}
      value={new Date(value)}
      mode={field}
      display={Platform.OS === 'ios' ? 'compact' : 'default'}
      minimumDate={field === 'date' ? new Date(now) : undefined}
      maximumDate={field === 'date' && latest !== undefined ? new Date(latest) : undefined}
      accentColor={palette.accent}
      is24Hour
      onValueChange={(_, next) => {
        setPicking(null);
        const chosen = next.getTime();
        onChange(field === 'date' ? withTimeOf(chosen, value) : withTimeOf(value, chosen));
      }}
      onDismiss={() => setPicking(null)}
    />
  );

  return (
    <>
      <ListSection>
        {Platform.OS === 'ios' ? (
          <>
            <View style={styles.inline}>
              <Text style={[font.body, { color: palette.text }]}>{dateLabel}</Text>
              {picker('date')}
            </View>
            <View style={styles.inline}>
              <Text style={[font.body, { color: palette.text }]}>{timeLabel}</Text>
              {picker('time')}
            </View>
          </>
        ) : (
          <>
            <ListRow
              testID={`${testID}-date`}
              icon="calendar"
              title={dateLabel}
              value={dateText}
              onPress={() => setPicking('date')}
            />
            <ListRow
              testID={`${testID}-time`}
              icon="clock"
              title={timeLabel}
              value={timeText}
              onPress={() => setPicking('time')}
            />
          </>
        )}
      </ListSection>
      {Platform.OS !== 'ios' && picking ? picker(picking) : null}
    </>
  );
}

const styles = StyleSheet.create({
  inline: {
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'space-between',
    paddingHorizontal: 16,
    paddingVertical: 8,
  },
});
