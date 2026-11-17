// The create sheet, from the "+" in iOS's tab bar: what can be made here, as the desktop's create
// menu (Android's floating button opens the same choices as its own menu).

import { type Href, router } from 'expo-router';
import { ScrollView, StyleSheet, View } from 'react-native';

import { t } from '../i18n';
import { createChoices } from '../state/create';
import { useStore } from '../state/react';
import { activeAccount } from '../state/selectors';
import { Divider, ListRow, ListSection } from '../ui/components';
import { CREATE_ITEMS, createHref } from '../ui/shell/create';
import { usePalette } from '../ui/theme';

/** Leaves this sheet for another screen. */
function go(href: Href) {
  router.back();
  router.push(href);
}

export default function CreateScreen() {
  const palette = usePalette();
  const account = useStore(activeAccount);

  if (!account) return null;

  return (
    <ScrollView style={{ backgroundColor: palette.bg }} contentContainerStyle={styles.content}>
      <ListSection title={t('create.menu')}>
        {createChoices(account).map((choice, i) => (
          <View key={choice}>
            {i > 0 ? <Divider inset={52} /> : null}
            <ListRow
              testID={`create-${choice}`}
              icon={CREATE_ITEMS[choice].icon}
              title={t(CREATE_ITEMS[choice].label)}
              onPress={() => go(createHref(choice, account))}
            />
          </View>
        ))}
      </ListSection>
    </ScrollView>
  );
}

const styles = StyleSheet.create({
  content: { paddingBottom: 24 },
});
