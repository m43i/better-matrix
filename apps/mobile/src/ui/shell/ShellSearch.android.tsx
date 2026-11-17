// The search under each tab's title, Android: a field at the top of the tab, under the header,
// as Material's search bar sits. What it finds shows below it in the tab's own content
// (`SearchResults`), so the list stays until something is typed.

import { useImperativeHandle, useRef, useState } from 'react';
import { Keyboard, StyleSheet, TextInput, type TextInputInstance, View } from 'react-native';

import { t } from '../../i18n';
import { font } from '../../theme/palette';
import { IconButton } from '../components';
import { Icon } from '../icons';
import { usePalette } from '../theme';
import type { ShellSearchProps } from './ShellSearch';

export function ShellSearch({ ref, placeholder, onChange, onSubmit }: ShellSearchProps) {
  const palette = usePalette();
  const field = useRef<TextInputInstance>(null);
  const [text, setText] = useState('');

  const change = (next: string) => {
    setText(next);
    onChange(next);
  };

  useImperativeHandle(ref, () => ({
    clear: () => {
      change('');
      field.current?.blur();
      Keyboard.dismiss();
    },
  }));

  return (
    <View style={[styles.field, { backgroundColor: palette.surface }]}>
      <Icon name="search" size={20} color={palette.textMuted} />
      <TextInput
        ref={field}
        testID="shell-search"
        value={text}
        onChangeText={change}
        onSubmitEditing={() => onSubmit(text)}
        placeholder={placeholder}
        placeholderTextColor={palette.textMuted}
        selectionColor={palette.accent}
        accessibilityRole="search"
        accessibilityLabel={placeholder}
        autoCapitalize="none"
        autoCorrect={false}
        returnKeyType="search"
        style={[font.body, styles.input, { color: palette.text }]}
      />
      {text !== '' ? (
        <IconButton
          icon="close"
          label={t('mobile.shell.search_clear')}
          onPress={() => change('')}
          size={32}
          color={palette.textMuted}
          testID="shell-search-clear"
        />
      ) : null}
    </View>
  );
}

const styles = StyleSheet.create({
  field: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 10,
    marginHorizontal: 16,
    marginTop: 4,
    marginBottom: 8,
    paddingLeft: 16,
    paddingRight: 6,
    height: 48,
    borderRadius: 24,
  },
  input: { flex: 1, paddingVertical: 0 },
});
