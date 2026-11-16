// A segmented control: Material's single-choice segmented buttons (Segments.tsx on iOS).

import { Host, SegmentedButton, SingleChoiceSegmentedButtonRow, Text } from '@expo/ui/jetpack-compose';
import { fillMaxWidth } from '@expo/ui/jetpack-compose/modifiers';
import { View } from 'react-native';

import { usePalette } from '../theme';
import type { SegmentsProps } from './Segments';

export function Segments<T extends string>({ options, selected, onChange, testID }: SegmentsProps<T>) {
  const palette = usePalette();
  return (
    <View testID={testID} style={{ alignSelf: 'stretch' }}>
      <Host matchContents={{ vertical: true }} colorScheme={palette.dark ? 'dark' : 'light'} seedColor={palette.accent}>
        <SingleChoiceSegmentedButtonRow modifiers={[fillMaxWidth()]}>
          {options.map((option) => (
            <SegmentedButton key={option.value} selected={option.value === selected} onClick={() => onChange(option.value)}>
              <SegmentedButton.Label>
                <Text>{option.label}</Text>
              </SegmentedButton.Label>
            </SegmentedButton>
          ))}
        </SingleChoiceSegmentedButtonRow>
      </Host>
    </View>
  );
}
