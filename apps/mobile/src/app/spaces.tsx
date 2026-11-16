// The space picker, opened from Home's title: Home and the spaces of the account on screen in
// the user's order (the desktop's rail), a dot on each with something new.

import type { MediaRef } from '@better-matrix/matrix-native/types';
import { router } from 'expo-router';
import { ScrollView, StyleSheet, View } from 'react-native';

import { t } from '../i18n';
import { routes } from '../routes';
import { setScope } from '../state/actions/core';
import { shallow, useAppStore, useStore } from '../state/react';
import { hasUnread, orderedSpaces, sameScope } from '../state/sections';
import { activeAccount } from '../state/selectors';
import type { Scope, State } from '../state/types';
import { Avatar } from '../ui/Avatar';
import { Divider, ListRow, ListSection } from '../ui/components';
import { Icon } from '../ui/icons';
import { WithDot } from '../ui/shell/AccountButton';
import { usePalette } from '../ui/theme';

interface ScopeEntry {
  /** The space's room ID; `null` for Home. */
  space: string | null;
  name: string;
  avatar: MediaRef | null;
  selected: boolean;
  unread: boolean;
}

function scopeEntries(s: State): ScopeEntry[] {
  const a = activeAccount(s);
  if (!a) return [];
  const home: ScopeEntry = { space: null, name: t('common.home'), avatar: null, selected: a.scope === 'home', unread: hasUnread(a, 'home') };
  return [
    home,
    ...orderedSpaces(a.spaces, a.sidebar).map((space): ScopeEntry => {
      const scope: Scope = { space: space.room_id };
      return {
        space: space.room_id,
        name: space.name,
        avatar: space.avatar,
        selected: sameScope(a.scope, scope),
        unread: hasUnread(a, scope),
      };
    }),
  ];
}

/** Equal when both lists hold entries with the same fields. */
function sameEntries<T extends object>(a: T[], b: T[]): boolean {
  return a.length === b.length && a.every((x, i) => shallow(x, b[i]));
}

export default function SpacesScreen() {
  const store = useAppStore();
  const palette = usePalette();
  const scopes = useStore(scopeEntries, sameEntries);
  const active = useStore((s) => s.active);
  if (!active) return null;
  return (
    <ScrollView style={{ backgroundColor: palette.bg }} contentContainerStyle={styles.content}>
      <ListSection title={t('mobile.shell.spaces')}>
        {scopes.map((entry, i) => (
          <View key={entry.space ?? 'home'}>
            {i > 0 ? <Divider inset={62} /> : null}
            <ListRow
              testID={`scope-${entry.space ?? 'home'}`}
              title={entry.name}
              leading={
                <WithDot on={!entry.selected && entry.unread} size={32}>
                  {entry.space ? (
                    <Avatar account={active} id={entry.space} name={entry.name} media={entry.avatar} size={32} />
                  ) : (
                    <View style={[styles.home, { backgroundColor: palette.hover }]}>
                      <Icon name="home" size={18} color={palette.text} />
                    </View>
                  )}
                </WithDot>
              }
              selected={entry.selected}
              chevron={false}
              onPress={() => {
                setScope(store, active, entry.space ? { space: entry.space } : 'home');
                router.back();
              }}
            />
          </View>
        ))}
      </ListSection>
      <ListSection>
        <ListRow
          testID="spaces-new"
          icon="plus"
          title={t('create.space')}
          onPress={() => {
            router.back();
            router.push(routes.newSpace());
          }}
        />
      </ListSection>
    </ScrollView>
  );
}

const styles = StyleSheet.create({
  content: { paddingBottom: 32 },
  home: { width: 32, height: 32, borderRadius: 7, alignItems: 'center', justifyContent: 'center' },
});
