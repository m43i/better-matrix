// The buttons of a modal's native header (new poll, new room or space, rename, recovery, invite):
// a text action at the right (Create, Save), and the way out. iOS says Cancel at the left, or
// Done at the right; Android shows a close icon at the left, as Material's full-screen dialogs
// do, with the gap to the title the header doesn't leave by itself.

import { Platform, Pressable, StyleSheet, Text, View } from 'react-native';

import { t } from '../i18n';
import { font } from '../theme/palette';
import { IconButton } from './components';
import { headerTint, usePalette } from './theme';

/** A text button for the native header: Create, Save, Done (and Cancel on iOS). */
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
      <Text style={[strong ? font.bodyStrong : font.body, { color: headerTint(palette) }]}>
        {title}
      </Text>
    </Pressable>
  );
}

/** The way out of a modal, at the header's left. */
export function HeaderCancel({ onPress, testID }: { onPress: () => void; testID: string }) {
  if (Platform.OS !== 'android')
    return <HeaderButton title={t('common.cancel')} onPress={onPress} testID={testID} />;

  return <CloseIcon onPress={onPress} testID={testID} label={t('common.cancel')} />;
}

/**
 * Closing a modal that saves as it goes (invite): Done at the right on iOS, the close icon at the
 * left on Android. Spread into the screen's options.
 */
export function doneOptions(onPress: () => void, testID: string) {
  return Platform.OS === 'android'
    ? { headerLeft: () => <CloseIcon onPress={onPress} testID={testID} label={t('common.done')} /> }
    : {
        headerRight: () => (
          <HeaderButton title={t('common.done')} onPress={onPress} strong testID={testID} />
        ),
      };
}

function CloseIcon({
  onPress,
  testID,
  label,
}: {
  onPress: () => void;
  testID: string;
  label: string;
}) {
  const palette = usePalette();

  return (
    <View style={styles.close}>
      <IconButton
        icon="close"
        label={label}
        onPress={onPress}
        size={40}
        color={palette.text}
        testID={testID}
      />
    </View>
  );
}

const styles = StyleSheet.create({
  // Material's 16 dp between the navigation icon and the title, past the icon's own padding.
  close: { marginLeft: -8, marginRight: 16 },
});
