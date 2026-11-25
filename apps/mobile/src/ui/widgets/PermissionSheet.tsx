// A widget asking for permissions, over its page: each permission in words with a switch (all on
// to start), then Allow (what is switched on) or Deny (nothing). A sheet on iOS, a full-screen
// dialog on Android; going back denies them all. The engine remembers the answer for the widget.

import { useState } from 'react';
import { Modal, Platform, ScrollView, StyleSheet, Text, View } from 'react-native';
import { SafeAreaProvider, SafeAreaView } from 'react-native-safe-area-context';

import { t } from '../../i18n';
import type { PermissionAsk } from '../../state/actions/widgets';
import { permissionText } from '../../state/widgets';
import { font } from '../../theme/palette';
import { Button, ListRow, ListSection } from '../components';
import { usePalette } from '../theme';

export function PermissionSheet({
  ask,
  onAnswer,
}: {
  ask: PermissionAsk;
  /** The capabilities granted; none denies them all. */
  onAnswer: (granted: string[]) => void;
}) {
  const palette = usePalette();

  const [allowed, setAllowed] = useState(
    () => new Set(ask.permissions.map((permission) => permission.capability)),
  );

  const toggle = (capability: string, on: boolean) =>
    setAllowed((current) => {
      const next = new Set(current);

      if (on) next.add(capability);
      else next.delete(capability);

      return next;
    });

  return (
    <Modal
      visible
      animationType="slide"
      presentationStyle="pageSheet"
      onRequestClose={() => onAnswer([])}
    >
      {/* A modal is a window of its own: it measures its own insets. iOS shows it as a sheet
          below the status bar; Android covers the screen, status bar included. */}
      <SafeAreaProvider>
        <SafeAreaView
          edges={
            Platform.OS === 'android'
              ? ['top', 'bottom', 'left', 'right']
              : ['bottom', 'left', 'right']
          }
          style={[styles.screen, { backgroundColor: palette.bg }]}
          testID="widget-permissions"
        >
          <ScrollView contentContainerStyle={styles.body}>
            <Text style={[font.header, styles.title, { color: palette.text }]}>
              {t('mobile.widgets.permissions_title', { widget: ask.widget })}
            </Text>
            <ListSection>
              {ask.permissions.map((permission) => (
                <ListRow
                  key={permission.capability}
                  testID={`widget-permission-${permission.capability}`}
                  title={permissionText(permission)}
                  titleLines={0}
                  toggle={{
                    value: allowed.has(permission.capability),
                    onChange: (on) => toggle(permission.capability, on),
                  }}
                />
              ))}
            </ListSection>
          </ScrollView>
          <View style={styles.buttons}>
            <Button
              testID="widget-permissions-allow"
              title={t('mobile.widgets.allow')}
              onPress={() =>
                onAnswer(
                  ask.permissions
                    .map((permission) => permission.capability)
                    .filter((capability) => allowed.has(capability)),
                )
              }
            />
            <Button
              testID="widget-permissions-deny"
              title={t('mobile.widgets.deny')}
              kind="secondary"
              onPress={() => onAnswer([])}
            />
          </View>
        </SafeAreaView>
      </SafeAreaProvider>
    </Modal>
  );
}

const styles = StyleSheet.create({
  screen: { flex: 1 },
  body: { paddingTop: 24, paddingBottom: 12 },
  title: { paddingHorizontal: 16, paddingBottom: 8 },
  buttons: { gap: 10, paddingHorizontal: 16, paddingVertical: 12 },
});
