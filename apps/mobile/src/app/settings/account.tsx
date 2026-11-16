// Account & security: whether this session is verified (and the ways to verify it), the
// recovery key, fetching keys from the backup, and resetting the identity when all else is lost.

import { router } from 'expo-router';

import { t } from '../../i18n';
import { routes } from '../../routes';
import { fetchRoomKeys } from '../../state/actions/accounts';
import { useAppStore } from '../../state/react';
import { Divider, ListRow, ListSection } from '../../ui/components';
import { AccountHeading, NoAccount, SettingsPage, StatusLine, useSettingsAccount } from '../../ui/settings/common';
import { usePalette } from '../../ui/theme';

export default function AccountScreen() {
  const store = useAppStore();
  const palette = usePalette();
  const account = useSettingsAccount();
  const title = t('mobile.accounts.security_title');
  if (!account) return <NoAccount title={title} />;
  const id = account.summary.id;
  const security = account.security ?? { session_verified: false, recovery: 'Unknown' as const };
  const verified = security.session_verified;

  const recovery = (() => {
    switch (security.recovery) {
      case 'Enabled':
        return { icon: 'key' as const, color: palette.success, text: t('recovery.enabled') };
      case 'Disabled':
        return { icon: 'key' as const, color: palette.warning, text: t('recovery.disabled') };
      case 'Incomplete':
        return { icon: 'key' as const, color: palette.warning, text: t('recovery.incomplete') };
      case 'Unknown':
        return { icon: 'key' as const, color: palette.textMuted, text: t('recovery.unknown') };
    }
  })();

  return (
    <SettingsPage title={title} testID="account-security">
      <AccountHeading account={account} />
      <ListSection title={t('account.session')} footer={verified ? t('account.session_hint') : t('account.unverified_hint')}>
        {verified ? (
          <StatusLine testID="session-verified" icon="verified" color={palette.success} text={t('account.verified')} />
        ) : (
          <>
            <StatusLine testID="session-unverified" icon="lock" color={palette.warning} text={t('account.unverified')} />
            <Divider />
            <ListRow
              testID="verify-device"
              icon="devices"
              title={t('account.verify_device')}
              onPress={() => router.push(routes.verify(id))}
            />
            <Divider inset={52} />
            <ListRow testID="verify-key" icon="key" title={t('account.verify_key')} onPress={() => router.push(routes.recovery(id))} />
          </>
        )}
      </ListSection>

      <ListSection title={t('recovery.title')} footer={t('recovery.hint')}>
        <StatusLine testID="recovery-status" icon={recovery.icon} color={recovery.color} text={recovery.text} />
        {security.recovery === 'Enabled' && verified ? (
          <>
            <Divider />
            <ListRow
              testID="recovery-reset"
              icon="key"
              title={t('recovery.reset')}
              onPress={() => router.push(routes.settingsRecoveryKey(id, true))}
            />
          </>
        ) : security.recovery === 'Disabled' ? (
          <>
            <Divider />
            <ListRow
              testID="recovery-set-up"
              icon="key"
              title={t('recovery.set_up')}
              onPress={() => router.push(routes.settingsRecoveryKey(id))}
            />
          </>
        ) : null}
      </ListSection>

      {verified ? (
        <ListSection footer={t('mobile.accounts.fetch_keys_hint')}>
          <ListRow
            testID="fetch-keys"
            icon="cloudDownload"
            title={t('mobile.accounts.fetch_keys')}
            chevron={false}
            onPress={() => fetchRoomKeys(store, id)}
          />
        </ListSection>
      ) : (
        <ListSection footer={t('reset.lost_everything')}>
          <ListRow
            testID="reset-identity"
            icon="alert"
            title={t('reset.title_short')}
            danger
            onPress={() => router.push(routes.settingsReset(id))}
          />
        </ListSection>
      )}
    </SettingsPage>
  );
}
