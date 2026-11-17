// The search under each tab's title, iOS: the native header's search bar, stacked under the
// large title (`placement="stacked"`), never hidden by scrolling. What it finds shows in the tab's
// own content (`SearchResults`), so the list behind stays until something is typed.

import { Stack } from 'expo-router';
import { type RefObject, useImperativeHandle, useRef } from 'react';
import type { SearchBarCommands } from 'react-native-screens';

import { usePalette } from '../theme';

/** What a screen does to its search: empty it once a result was picked. */
export interface ShellSearchHandle {
  clear(): void;
}

export interface ShellSearchProps {
  ref: RefObject<ShellSearchHandle | null>;
  placeholder: string;
  onChange: (text: string) => void;
  /** The search key: opens a typed link. */
  onSubmit: (text: string) => void;
}

export function ShellSearch({ ref, placeholder, onChange, onSubmit }: ShellSearchProps) {
  const palette = usePalette();
  const bar = useRef<SearchBarCommands>(null);
  useImperativeHandle(ref, () => ({
    clear: () => {
      bar.current?.cancelSearch();
      onChange('');
    },
  }));

  return (
    <Stack.SearchBar
      ref={bar}
      placement="stacked"
      hideWhenScrolling={false}
      obscureBackground={false}
      autoCapitalize="none"
      placeholder={placeholder}
      onChangeText={(e) => onChange(e.nativeEvent.text)}
      onSearchButtonPress={(e) => onSubmit(e.nativeEvent.text)}
      onCancelButtonPress={() => onChange('')}
      textColor={palette.text}
      tintColor={palette.accent}
    />
  );
}
