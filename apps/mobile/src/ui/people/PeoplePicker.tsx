// A screen that finds people: the native header's search bar over the directory's results, the
// people the user shares rooms with suggested before anything is typed, and a typed user ID
// offered as it is. The invite and new-chat modals are built on it.

import type { AccountId, Person } from '@better-matrix/matrix-native/types';
import { Stack } from 'expo-router';
import type { ReactNode } from 'react';
import { ActivityIndicator, FlatList, StyleSheet, Text, View } from 'react-native';

import { t } from '../../i18n';
import { busy, candidates, query } from '../../state/people';
import { font } from '../../theme/palette';
import { Divider } from '../components';
import { usePalette } from '../theme';
import { PersonRow } from './rows';
import { usePeopleSearch } from './usePeopleSearch';

export function PeoplePicker({
  account,
  placeholder,
  onPick,
  trailing,
  testID,
}: {
  account: AccountId;
  placeholder: string;
  onPick: (person: Person) => void;
  /** What follows a person: a status, or nothing when pressing them acts. */
  trailing?: (person: Person) => ReactNode;
  testID: string;
}) {
  const palette = usePalette();
  const [search, type] = usePeopleSearch(account);
  const people = candidates(search, account);
  const typing = query(search) !== '';
  const loading = busy(search);
  return (
    <>
      <Stack.SearchBar
        placeholder={placeholder}
        autoCapitalize="none"
        autoFocus
        hideWhenScrolling={false}
        onChangeText={(event) => type(event.nativeEvent.text)}
        tintColor={palette.accent}
        textColor={palette.text}
        hintTextColor={palette.textMuted}
        headerIconColor={palette.text}
      />
      <FlatList
        testID={testID}
        data={people}
        keyExtractor={(person) => person.id}
        contentInsetAdjustmentBehavior="automatic"
        keyboardShouldPersistTaps="handled"
        keyboardDismissMode="on-drag"
        ItemSeparatorComponent={() => <Divider inset={68} />}
        ListHeaderComponent={
          !typing && people.length > 0 ? (
            <Text style={[styles.section, font.section, { color: palette.textMuted }]}>{t('create.suggested')}</Text>
          ) : undefined
        }
        ListEmptyComponent={
          <View style={styles.empty}>
            {loading ? <ActivityIndicator color={palette.textMuted} /> : null}
            {typing ? (
              <Text style={[font.body, { color: palette.textMuted, textAlign: 'center' }]}>
                {loading ? t('search.searching') : t('search.nothing')}
              </Text>
            ) : null}
          </View>
        }
        renderItem={({ item }) => (
          <PersonRow
            account={account}
            person={item}
            trailing={trailing?.(item)}
            onPress={() => onPick(item)}
            testID={`${testID}-${item.id}`}
          />
        )}
      />
    </>
  );
}

const styles = StyleSheet.create({
  section: { paddingHorizontal: 16, paddingTop: 16, paddingBottom: 6 },
  empty: { alignItems: 'center', gap: 10, padding: 32 },
});
