// The frame of the room forms (new room, new space, rename): a title, Cancel and a confirming
// button in the native header, and the fields below. The confirming button is off until the
// form is valid, and spins while the engine works.

import { Stack, useRouter } from 'expo-router';
import type { ReactNode } from 'react';
import { ActivityIndicator, ScrollView, StyleSheet } from 'react-native';

import { HeaderButton, HeaderCancel } from '../HeaderButtons';
import { usePalette } from '../theme';

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
            <HeaderCancel onPress={() => router.back()} testID={`${testID}-cancel`} />
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
