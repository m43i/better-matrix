// The frame of the room forms (new room, new space, rename): a title, Cancel and a confirming
// button in the native header, and the fields below. The confirming button is off until the
// form is valid, and spins while the engine works.

import { Stack, useRouter } from 'expo-router';
import type { ReactNode } from 'react';
import { ActivityIndicator, Platform, Pressable, ScrollView, StyleSheet, Text } from 'react-native';

import { t } from '../../i18n';
import { font } from '../../theme/palette';
import { usePalette } from '../theme';

/** A text button for the native header: Cancel, Create, Save, Done. */
export function HeaderButton({
  title,
  onPress,
  disabled = false,
  strong = false,
  testID,
}: {
  title: string;
  onPress: () => void;
  disabled?: boolean;
  strong?: boolean;
  testID: string;
}) {
  const palette = usePalette();

  return (
    <Pressable
      testID={testID}
      onPress={onPress}
      disabled={disabled}
      hitSlop={10}
      accessibilityRole="button"
      accessibilityState={{ disabled }}
      style={({ pressed }) => ({
        opacity: disabled ? 0.4 : pressed && Platform.OS === 'ios' ? 0.6 : 1,
        paddingHorizontal: 4,
      })}
    >
      <Text style={[strong ? font.bodyStrong : font.body, { color: palette.accent }]}>{title}</Text>
    </Pressable>
  );
}

export function RoomForm({
  title,
  confirm,
  valid,
  busy,
  onSubmit,
  children,
  testID,
}: {
  title: string;
  confirm: string;
  valid: boolean;
  busy: boolean;
  onSubmit: () => void;
  children: ReactNode;
  testID: string;
}) {
  const router = useRouter();
  const palette = usePalette();

  return (
    <>
      <Stack.Screen
        options={{
          title,
          headerLeft: () => (
            <HeaderButton
              title={t('common.cancel')}
              onPress={() => router.back()}
              testID={`${testID}-cancel`}
            />
          ),
          headerRight: () =>
            busy ? (
              <ActivityIndicator color={palette.accent} />
            ) : (
              <HeaderButton
                title={confirm}
                onPress={onSubmit}
                disabled={!valid}
                strong
                testID={`${testID}-submit`}
              />
            ),
        }}
      />
      <ScrollView
        testID={testID}
        contentInsetAdjustmentBehavior="automatic"
        keyboardShouldPersistTaps="handled"
        contentContainerStyle={styles.content}
      >
        {children}
      </ScrollView>
    </>
  );
}

const styles = StyleSheet.create({
  content: { padding: 16, gap: 18 },
});
