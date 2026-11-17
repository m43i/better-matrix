// The create menu on Android: Material 3's floating action button menu at the bottom right of
// each tab. The button opens what can be made here (the desktop's create menu: a chat, a room,
// a space, a section) as a column of labelled buttons above it and turns into a close button;
// the scrim, the back button or a pick closes it.

import * as Haptics from 'expo-haptics';
import { type Href, router, useFocusEffect } from 'expo-router';
import { useCallback, useEffect, useState } from 'react';
import { BackHandler, Pressable, StyleSheet, Text, View } from 'react-native';
import Animated, {
  FadeIn,
  FadeInDown,
  FadeOut,
  FadeOutDown,
  useAnimatedStyle,
  withSpring,
} from 'react-native-reanimated';

import { t } from '../../i18n';
import { createChoices } from '../../state/create';
import { useStore } from '../../state/react';
import { activeAccount } from '../../state/selectors';
import { font } from '../../theme/palette';
import { Icon } from '../icons';
import { usePalette } from '../theme';
import { CREATE_ITEMS, createHref } from './create';

export function CreateFab() {
  const palette = usePalette();
  const account = useStore(activeAccount);
  const [open, setOpen] = useState(false);

  // Leaving the tab closes the menu, so a hidden tab never holds an open menu (or Back).
  useFocusEffect(useCallback(() => () => setOpen(false), []));

  // Back closes the menu before it leaves the screen.
  useEffect(() => {
    if (!open) return;

    const subscription = BackHandler.addEventListener('hardwareBackPress', () => {
      setOpen(false);

      return true;
    });

    return () => subscription.remove();
  }, [open]);

  const turn = useAnimatedStyle(() => ({
    transform: [{ rotate: withSpring(open ? '45deg' : '0deg', { damping: 18, stiffness: 220 }) }],
  }));

  const fab = useAnimatedStyle(() => ({
    borderRadius: withSpring(open ? 28 : 16, { damping: 18, stiffness: 220 }),
  }));

  if (!account) return null;
  const choices = createChoices(account);

  const toggle = () => {
    void Haptics.impactAsync(Haptics.ImpactFeedbackStyle.Light);
    setOpen((was) => !was);
  };

  const go = (href: Href) => {
    setOpen(false);
    router.push(href);
  };

  return (
    <View style={StyleSheet.absoluteFill} pointerEvents="box-none">
      {open ? (
        <Animated.View entering={FadeIn} exiting={FadeOut} style={StyleSheet.absoluteFill}>
          <Pressable
            testID="create-fab-scrim"
            accessibilityRole="button"
            accessibilityLabel={t('common.close')}
            onPress={() => setOpen(false)}
            style={[StyleSheet.absoluteFill, styles.scrim, { backgroundColor: palette.scrim }]}
          />
        </Animated.View>
      ) : null}
      <View style={styles.anchor} pointerEvents="box-none">
        {open ? (
          <View style={styles.items}>
            {choices.map((choice, i) => (
              <Animated.View
                key={choice}
                entering={FadeInDown.delay((choices.length - 1 - i) * 30).springify()}
                exiting={FadeOutDown.duration(120)}
              >
                <Pressable
                  testID={`create-fab-${choice}`}
                  accessibilityRole="button"
                  onPress={() => go(createHref(choice, account))}
                  android_ripple={{ color: palette.hover, foreground: true }}
                  style={[styles.item, { backgroundColor: palette.accent }]}
                >
                  <Icon name={CREATE_ITEMS[choice].icon} size={22} color={palette.onAccent} />
                  <Text style={[font.bodyStrong, { color: palette.onAccent }]}>
                    {t(CREATE_ITEMS[choice].label)}
                  </Text>
                </Pressable>
              </Animated.View>
            ))}
          </View>
        ) : null}
        <Animated.View style={[styles.fab, fab, { backgroundColor: palette.accent }]}>
          <Pressable
            testID="create-fab"
            accessibilityRole="button"
            accessibilityLabel={open ? t('common.close') : t('create.menu')}
            accessibilityState={{ expanded: open }}
            onPress={toggle}
            android_ripple={{ color: palette.hover, foreground: true }}
            style={styles.press}
          >
            <Animated.View style={turn}>
              <Icon name="plus" size={26} color={palette.onAccent} />
            </Animated.View>
          </Pressable>
        </Animated.View>
      </View>
    </View>
  );
}

const styles = StyleSheet.create({
  scrim: { opacity: 0.5 },
  anchor: {
    position: 'absolute',
    right: 16,
    bottom: 16,
    alignItems: 'flex-end',
    gap: 12,
  },
  items: { alignItems: 'flex-end', gap: 8 },
  item: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 12,
    height: 56,
    paddingHorizontal: 20,
    borderRadius: 28,
    overflow: 'hidden',
    elevation: 3,
  },
  fab: { width: 56, height: 56, overflow: 'hidden', elevation: 6 },
  press: { flex: 1, alignItems: 'center', justifyContent: 'center' },
});
