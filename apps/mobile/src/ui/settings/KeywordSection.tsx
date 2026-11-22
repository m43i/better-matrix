// An account's notification keywords on the Notifications page: the words that notify the user
// wherever they are written (push rules on the account's server), each removable, and a field
// adding another, as the desktop's settings list them.

import type { AccountId } from '@better-matrix/matrix-native/types';
import { Fragment, useState } from 'react';
import { Alert, StyleSheet, View } from 'react-native';

import { t } from '../../i18n';
import { addKeyword, removeKeyword } from '../../state/actions/accounts';
import { useAppStore, useStore } from '../../state/react';
import { Button, Divider, ListRow, ListSection, TextField } from '../components';

export function KeywordSection({ account }: { account: AccountId }) {
  const store = useAppStore();
  const keywords = useStore((s) => s.accounts.find((a) => a.summary.id === account)?.keywords);
  const [draft, setDraft] = useState('');

  const add = () => {
    if (addKeyword(store, account, draft)) setDraft('');
  };

  const remove = (keyword: string) => {
    Alert.alert(t('settings.keyword_remove', { keyword }), undefined, [
      { text: t('common.cancel'), style: 'cancel' },
      {
        text: t('common.remove'),
        style: 'destructive',
        onPress: () => removeKeyword(store, account, keyword),
      },
    ]);
  };

  return (
    <ListSection title={t('settings.keywords')} footer={t('settings.keywords_hint')}>
      {keywords === null || keywords === undefined ? (
        <ListRow title={t('profile.loading')} />
      ) : (
        keywords.map((keyword, index) => (
          <Fragment key={keyword}>
            {index > 0 ? <Divider inset={52} /> : null}
            <ListRow
              testID={`keyword-${account}-${keyword}`}
              icon="keyword"
              title={keyword}
              value={t('common.remove')}
              chevron={false}
              onPress={() => remove(keyword)}
            />
          </Fragment>
        ))
      )}
      {keywords && keywords.length > 0 ? <Divider /> : null}
      <View style={styles.form}>
        <TextField
          testID={`keyword-new-${account}`}
          value={draft}
          onChangeText={setDraft}
          placeholder={t('settings.keyword_placeholder')}
          autoCapitalize="none"
          autoCorrect={false}
          returnKeyType="done"
          onSubmitEditing={add}
        />
        <Button
          testID={`keyword-add-${account}`}
          title={t('settings.keyword_add')}
          kind="secondary"
          disabled={!draft.trim()}
          onPress={add}
        />
      </View>
    </ListSection>
  );
}

const styles = StyleSheet.create({
  form: { padding: 16, gap: 12 },
});
