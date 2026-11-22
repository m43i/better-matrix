// Leaving a space (modal, from its sheet): which of its joined rooms to leave too, none chosen at
// first, with all or none at a tap; Leave leaves those, then the space.

import { Stack, useFocusEffect, useLocalSearchParams, useRouter } from 'expo-router';
import { useCallback, useState } from 'react';
import { ActivityIndicator, ScrollView, StyleSheet, Text, View } from 'react-native';

import { t } from '../i18n';
import { leaveSpace } from '../state/actions/admin';
import { joinedChildren } from '../state/admin';
import { shallow, useAppStore, useStore } from '../state/react';
import { account as accountOf, space as spaceOf } from '../state/selectors';
import { font } from '../theme/palette';
import { Avatar } from '../ui/Avatar';
import { Divider, ListRow, ListSection } from '../ui/components';
import { HeaderButton, HeaderCancel } from '../ui/HeaderButtons';
import { usePalette } from '../ui/theme';
import { useShown } from '../ui/useShown';

export default function LeaveSpaceScreen() {
  const { account, space: spaceId } = useLocalSearchParams<{ account: string; space: string }>();
  const store = useAppStore();
  const router = useRouter();
  const palette = usePalette();
  const space = useStore((s) => spaceOf(s, account, spaceId));
  const rooms = useStore((s) => accountOf(s, account)?.rooms ?? [], shallow);
  const spaces = useStore((s) => accountOf(s, account)?.spaces ?? [], shallow);
  const [chosen, setChosen] = useState<ReadonlySet<string>>(new Set());
  const [busy, setBusy] = useState(false);
  const shown = useShown();
  const gone = space === undefined;

  // Come back to after the space was left (another screen covered it meanwhile): nothing is left
  // to choose, so it closes.
  useFocusEffect(
    useCallback(() => {
      if (gone && router.canGoBack()) router.back();
    }, [gone, router]),
  );

  if (!space) return null;
  const joined = joinedChildren(space, rooms, spaces);
  const all = joined.length > 0 && joined.every((c) => chosen.has(c.roomId));

  const toggle = (roomId: string) =>
    setChosen((current) => {
      const next = new Set(current);

      if (!next.delete(roomId)) next.add(roomId);

      return next;
    });

  const leave = () => {
    if (busy) return;
    setBusy(true);
    leaveSpace(store, account, space, chosen).then(
      () => {
        // Leaving that ends after the user went elsewhere leaves them there.
        if (shown.current) router.dismissAll();
      },
      () => setBusy(false),
    );
  };

  return (
    <>
      <Stack.Screen
        options={{
          title: t('mobile.admin.leave_title', { space: space.name }),
          headerLeft: () => (
            <HeaderCancel onPress={() => router.back()} testID="leave-space-cancel" />
          ),
          headerRight: () =>
            busy ? (
              <ActivityIndicator color={palette.accent} />
            ) : (
              <HeaderButton
                title={t('mobile.admin.leave')}
                onPress={leave}
                strong
                testID="leave-space-submit"
              />
            ),
        }}
      />
      <ScrollView
        testID="leave-space"
        style={{ backgroundColor: palette.bg }}
        contentContainerStyle={styles.content}
        contentInsetAdjustmentBehavior="automatic"
      >
        <Text style={[font.body, styles.intro, { color: palette.textMuted }]}>
          {t('mobile.admin.leave_body')}
        </Text>
        {joined.length > 0 ? (
          <ListSection title={t('mobile.admin.leave_rooms')}>
            <ListRow
              testID="leave-space-all"
              title={all ? t('mobile.admin.select_none') : t('mobile.admin.select_all')}
              chevron={false}
              onPress={() => setChosen(all ? new Set() : new Set(joined.map((c) => c.roomId)))}
            />
            {joined.map((child) => (
              <View key={child.roomId}>
                <Divider inset={58} />
                <ListRow
                  testID={`leave-space-${child.roomId}`}
                  title={child.name}
                  subtitle={child.space ? t('mobile.admin.subspace') : undefined}
                  leading={
                    <Avatar
                      account={account}
                      id={child.roomId}
                      name={child.name}
                      media={null}
                      size={28}
                    />
                  }
                  checked={chosen.has(child.roomId)}
                  chevron={false}
                  onPress={() => toggle(child.roomId)}
                />
              </View>
            ))}
          </ListSection>
        ) : null}
      </ScrollView>
    </>
  );
}

const styles = StyleSheet.create({
  content: { paddingBottom: 32 },
  intro: { marginHorizontal: 20, marginTop: 16 },
});
