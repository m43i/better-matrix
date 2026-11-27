import { Stack, useLocalSearchParams } from 'expo-router';

import { t } from '../i18n';
import { useStore } from '../state/react';
import { account as accountOf } from '../state/selectors';
import { StatusMessageEditor } from '../ui/accounts/StatusMessageEditor';

export default function StatusMessageScreen() {
  const { account } = useLocalSearchParams<{ account: string }>();
  const data = useStore((state) => accountOf(state, account));

  return (
    <>
      <Stack.Screen options={{ title: t('status_message.set') }} />
      {data ? <StatusMessageEditor key={account} account={data} /> : null}
    </>
  );
}
