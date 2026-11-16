// A segmented control, the platform's own: SwiftUI's segmented picker here (iOS), Material's
// segmented buttons on Android (Segments.android.tsx).

import { Host, Picker, Text } from '@expo/ui/swift-ui';
import { pickerStyle, tag } from '@expo/ui/swift-ui/modifiers';

import { usePalette } from '../theme';

export interface SegmentsProps<T extends string> {
  options: ReadonlyArray<{ value: T; label: string }>;
  selected: T;
  onChange: (value: T) => void;
  testID?: string;
}

export function Segments<T extends string>({ options, selected, onChange, testID }: SegmentsProps<T>) {
  const palette = usePalette();
  return (
    <Host matchContents={{ vertical: true }} colorScheme={palette.dark ? 'dark' : 'light'} style={{ alignSelf: 'stretch' }} testID={testID}>
      <Picker<T> selection={selected} onSelectionChange={onChange} modifiers={[pickerStyle('segmented')]}>
        {options.map((option) => (
          <Text key={option.value} modifiers={[tag(option.value)]}>
            {option.label}
          </Text>
        ))}
      </Picker>
    </Host>
  );
}
