// Account & security: whether this session is verified (and the ways to verify it), the
// recovery key, fetching keys from the backup, the offline device, exporting and importing
// message keys, resetting the identity when all else is lost, and the account itself: its
// password and deactivating it (on the account page for OAuth accounts).

import type { Security } from '@better-matrix/matrix-native/types';
import { router } from 'expo-router';
import { useState } from 'react';
import { Alert } from 'react-native';

import { t, tp } from '../../i18n';
import { routes } from '../../routes';
import { fetchRoomKeys, openAccountPage } from '../../state/actions/accounts';
import {
  deactivateAccount,
  importRoomKeysAsking,
  setOfflineDevice,
  turnOnOfflineDevice,
} from '../../state/actions/security';
import { useAppStore } from '../../state/react';
import { addNotice } from '../../state/reducer';
import { openInBrowser } from '../../ui/accounts/browser';
import { usePrompt } from '../../ui/accounts/Prompt';
import { Divider, ListRow, ListSection } from '../../ui/components';
import {
  AccountHeading,
  NoAccount,
  SettingsPage,
  StatusLine,
  useSettingsAccount,
} from '../../ui/settings/common';
import { pickKeyFile } from '../../ui/settings/keys';
import { usePalette } from '../../ui/theme';

const UNKNOWN: Security = {
  session_verified: false,
  recovery: 'Unknown',
  offline_device: 'Unknown',
};

export default function AccountScreen() {
  const store = useAppStore();
  const palette = usePalette();
  const account = useSettingsAccount();
  const prompt = usePrompt();
  // The offline device's switch while it turns on or off.
  const [switching, setSwitching] = useState<boolean | null>(null);
  const title = t('mobile.accounts.security_title');

  if (!account) return <NoAccount title={title} />;
  const { summary } = account;
  const id = summary.id;
  const oauth = summary.sign_in === 'OAuth';
  const security = account.security ?? UNKNOWN;
  const verified = security.session_verified;

  const notice = (cause: unknown) =>
    store.update((s) => addNotice(s, cause instanceof Error ? cause.message : String(cause), id));

  const switchOfflineDevice = (on: boolean) => {
    if (switching !== null) return;
    setSwitching(on);

    const done = on
      ? turnOnOfflineDevice(store, id, (error) =>
          prompt.ask({
            title: t('mobile.accounts.offline_key_title'),
            message: t('mobile.accounts.offline_key_message'),
            error,
            confirm: t('mobile.accounts.offline_turn_on'),
            secure: true,
            placeholder: t('recovery.key_placeholder'),
          }),
        )
      : setOfflineDevice(store, id, false);

    void done.catch(notice).finally(() => setSwitching(null));
  };

  const importKeys = async () => {
    try {
      const path = await pickKeyFile();

      if (path === null) return;

      const found = await importRoomKeysAsking(store, id, path, (error) =>
        prompt.ask({
          title: t('mobile.accounts.import_keys'),
          message: t('mobile.accounts.import_message'),
          error,
          confirm: t('mobile.accounts.import_action'),
          secure: true,
          placeholder: t('mobile.accounts.passphrase'),
        }),
      );

      if (found)
        Alert.alert(
          t('mobile.accounts.imported_title'),
          tp('mobile.accounts.imported', found.total, { imported: found.imported }),
        );
    } catch (cause) {
      notice(cause);
    }
  };

  const changePassword = () => {
    if (oauth) openAccountPage(store, id, (url) => openInBrowser(store, url)).catch(notice);
    else router.push(routes.settingsPassword(id));
  };

  const deactivate = (erase: boolean) =>
    deactivateAccount(store, id, erase, {
      askPassword: (error) =>
        prompt.ask({
          title: t('mobile.accounts.deactivate'),
          message: t('mobile.accounts.deactivate_password'),
          error,
          confirm: t('mobile.accounts.deactivate'),
          secure: true,
          destructive: true,
          placeholder: t('sign_in.password'),
        }),
      openUrl: (url) => openInBrowser(store, url),
    }).catch(notice);

  const askDeactivate = () => {
    // An OAuth account deactivates on its account page, which asks itself.
    if (oauth) {
      void deactivate(false);

      return;
    }

    Alert.alert(
      t('mobile.accounts.deactivate_title', { account: id }),
      t('mobile.accounts.deactivate_body'),
      [
        { text: t('common.cancel'), style: 'cancel' },
        {
          text: t('mobile.accounts.deactivate_erase'),
          style: 'destructive',
          onPress: () => void deactivate(true),
        },
        {
          text: t('mobile.accounts.deactivate'),
          style: 'destructive',
          onPress: () => void deactivate(false),
        },
      ],
    );
  };

  const offline = security.offline_device;

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
      <ListSection
        title={t('account.session')}
        footer={verified ? t('account.session_hint') : t('account.unverified_hint')}
      >
        {verified ? (
          <StatusLine
            testID="session-verified"
            icon="verified"
            color={palette.success}
            text={t('account.verified')}
          />
        ) : (
          <>
            <StatusLine
              testID="session-unverified"
              icon="lock"
              color={palette.warning}
              text={t('account.unverified')}
            />
            <Divider />
            <ListRow
              testID="verify-device"
              icon="devices"
              title={t('account.verify_device')}
              onPress={() => router.push(routes.verify(id))}
            />
            <Divider inset={52} />
            <ListRow
              testID="verify-key"
              icon="key"
              title={t('account.verify_key')}
              onPress={() => router.push(routes.recovery(id))}
            />
          </>
        )}
      </ListSection>

      <ListSection title={t('recovery.title')} footer={t('recovery.hint')}>
        <StatusLine
          testID="recovery-status"
          icon={recovery.icon}
          color={recovery.color}
          text={recovery.text}
        />
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

      {offline === 'On' || offline === 'Off' ? (
        <ListSection
          title={t('mobile.accounts.offline_title')}
          footer={t('mobile.accounts.offline_hint')}
        >
          <ListRow
            testID="offline-device"
            icon="cloud"
            title={t('mobile.accounts.offline_switch')}
            toggle={{ value: switching ?? offline === 'On', onChange: switchOfflineDevice }}
          />
        </ListSection>
      ) : null}

      <ListSection title={t('mobile.accounts.keys_title')} footer={t('mobile.accounts.keys_hint')}>
        <ListRow
          testID="export-keys"
          icon="upload"
          title={t('mobile.accounts.export_keys')}
          onPress={() => router.push(routes.settingsExportKeys(id))}
        />
        <Divider inset={52} />
        <ListRow
          testID="import-keys"
          icon="download"
          title={t('mobile.accounts.import_keys')}
          chevron={false}
          onPress={() => void importKeys()}
        />
      </ListSection>

      <ListSection
        title={t('mobile.accounts.account_section')}
        footer={t('mobile.accounts.deactivate_hint')}
      >
        <ListRow
          testID="change-password"
          icon="lock"
          title={t('mobile.accounts.password_title')}
          subtitle={oauth ? t('mobile.accounts.on_account_page') : undefined}
          onPress={changePassword}
        />
        <Divider inset={52} />
        <ListRow
          testID="deactivate-account"
          icon="trash"
          title={t('mobile.accounts.deactivate')}
          subtitle={oauth ? t('mobile.accounts.on_account_page') : undefined}
          danger
          chevron={false}
          onPress={askDeactivate}
        />
      </ListSection>
      {prompt.element}
    </SettingsPage>
  );
}
