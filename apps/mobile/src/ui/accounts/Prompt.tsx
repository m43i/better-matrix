// A prompt for one line of text (a password, a session's name), awaited by the action that needs
// it: iOS's own alert with a text field, and on Android (which has none) a small dialog in the
// same shape. The screen renders `element` and calls `ask`.

import { useCallback, useEffect, useRef, useState } from 'react';
import { Alert, KeyboardAvoidingView, Modal, Platform, StyleSheet, Text, View } from 'react-native';

import { t } from '../../i18n';
import { font } from '../../theme/palette';
import { Button, TextField } from '../components';
import { usePalette } from '../theme';
import { ModalNotices } from '../Notices';

export interface PromptOptions {
  title: string;
  message?: string;
  /** Why the last answer was refused. */
  error?: string | null;
  /** The confirming button's label. */
  confirm: string;
  /** A password: hidden as it is typed. */
  secure?: boolean;
  /** The confirming button removes or replaces something. */
  destructive?: boolean;
  defaultValue?: string;
  placeholder?: string;
}

interface Asking {
  id: number;
  options: PromptOptions;
  resolve: (value: string | null) => void;
}

let nextId = 1;

/** `ask` shows the prompt and resolves with what was entered, or `null` when cancelled. */
export function usePrompt() {
  const [asking, setAsking] = useState<Asking | null>(null);
  const pending = useRef<Asking | null>(null);

  // A screen that goes away answers its open prompt with "cancelled".
  useEffect(() => () => pending.current?.resolve(null), []);

  const ask = useCallback((options: PromptOptions) => {
    pending.current?.resolve(null);

    return new Promise<string | null>((resolve) => {
      const entry: Asking = {
        id: nextId++,
        options,
        resolve: (value) => {
          if (pending.current === entry) pending.current = null;
          setAsking((current) => (current === entry ? null : current));
          resolve(value);
        },
      };

      pending.current = entry;

      if (Platform.OS === 'ios') promptIos(entry);
      else setAsking(entry);
    });
  }, []);

  return { ask, element: asking ? <PromptDialog key={asking.id} asking={asking} /> : null };
}

function promptIos({ options, resolve }: Pick<Asking, 'options' | 'resolve'>): void {
  const message = [options.message, options.error].filter(Boolean).join('\n\n');
  Alert.prompt(
    options.title,
    message || undefined,
    [
      { text: t('common.cancel'), style: 'cancel', onPress: () => resolve(null) },
      {
        text: options.confirm,
        style: options.destructive ? 'destructive' : 'default',
        isPreferred: true,
        onPress: (value?: string) => {
          // Nothing entered: ask again rather than send an empty answer.
          if (!value?.trim()) promptIos({ options: { ...options, error: null }, resolve });
          else resolve(value);
        },
      },
    ],
    options.secure ? 'secure-text' : 'plain-text',
    options.defaultValue ?? '',
  );
}

function PromptDialog({ asking: { options, resolve } }: { asking: Asking }) {
  const palette = usePalette();
  const [value, setValue] = useState(options.defaultValue ?? '');

  const submit = () => {
    if (value.trim()) resolve(value);
  };

  return (
    <Modal
      transparent
      animationType="fade"
      onRequestClose={() => resolve(null)}
      statusBarTranslucent
    >
      <KeyboardAvoidingView
        behavior="padding"
        style={[styles.scrim, { backgroundColor: palette.scrim }]}
      >
        <View style={[styles.card, { backgroundColor: palette.bg }]} testID="prompt">
          <Text style={[font.header, { color: palette.text }]}>{options.title}</Text>
          {options.message ? (
            <Text style={[font.body, { color: palette.textMuted }]}>{options.message}</Text>
          ) : null}
          <TextField
            testID="prompt-field"
            value={value}
            onChangeText={setValue}
            onSubmitEditing={submit}
            secureTextEntry={options.secure}
            placeholder={options.placeholder}
            autoFocus
            autoCapitalize="none"
            autoCorrect={false}
            error={options.error}
            returnKeyType="done"
          />
          <View style={styles.buttons}>
            <Button
              testID="prompt-cancel"
              title={t('common.cancel')}
              kind="plain"
              onPress={() => resolve(null)}
            />
            <Button
              testID="prompt-confirm"
              title={options.confirm}
              kind={options.destructive ? 'danger' : 'primary'}
              disabled={!value.trim()}
              onPress={submit}
            />
          </View>
        </View>
      </KeyboardAvoidingView>
      <ModalNotices />
    </Modal>
  );
}

const styles = StyleSheet.create({
  scrim: { flex: 1, justifyContent: 'center', padding: 24 },
  card: { borderRadius: 16, padding: 20, gap: 14 },
  buttons: { flexDirection: 'row', justifyContent: 'flex-end', gap: 8 },
});
