// Notifications: whether this device gets pushes (for every account, or per account), how
// incoming calls ring on this device, and each account's default notification mode (push rules
// live on each account's server), as the desktop's page.

import * as Notifications from 'expo-notifications';
import { Fragment, useEffect, useState } from 'react';

import { t } from '../../i18n';
import { setDefaultNotificationMode } from '../../state/actions/accounts';
import { updateSettings } from '../../state/actions/core';
import { accountPushOn, setAccountPush, setPushEnabled } from '../../state/actions/push';
import { pushUnavailable } from '../../engine/pushConfig';
import { shallow, useAppStore, useStore } from '../../state/react';
import { Avatar } from '../../ui/Avatar';
import { Divider, ListRow, ListSection } from '../../ui/components';
import { NOTIFICATION_MODES } from '../../ui/settings/choices';
import { SettingsPage } from '../../ui/settings/common';

/** Whether the system lets the app notify; null until known. */
function useNotificationsAllowed(): boolean | null {
  const [allowed, setAllowed] = useState<boolean | null>(null);
  useEffect(() => {
    let live = true;
    Notifications.getPermissionsAsync()
      .then((permission) => {
        if (live) setAllowed(permission.granted);
      })
      .catch(() => undefined);
    return () => {
      live = false;
    };
  }, []);
  return allowed;
}

export default function NotificationsScreen() {
  const store = useAppStore();
  const ringSound = useStore((s) => s.settings.ringSound);
  const accounts = useStore((s) => s.accounts.map((a) => a.summary), shallow);
  const push = useStore((s) => ({ pushEnabled: s.settings.pushEnabled, pushOff: s.settings.pushOff }), shallow);
  const pushReady = pushUnavailable() === null;
  const allowed = useNotificationsAllowed();
  const pushFooter = !pushReady
    ? t('mobile.push.unavailable')
    : allowed === false
      ? t('mobile.push.denied')
      : t('mobile.push.device_hint');
  return (
    <SettingsPage title={t('settings.notifications')} testID="notification-settings">
      <ListSection footer={pushFooter}>
        <ListRow
          testID="push-device"
          icon="bell"
          title={t('mobile.push.device')}
          toggle={pushReady ? { value: push.pushEnabled, onChange: (on) => setPushEnabled(store, on) } : undefined}
        />
      </ListSection>
      <ListSection title={t('settings.ring')} footer={ringSound ? t('settings.ring_on_hint') : t('settings.ring_off_hint')}>
        <ListRow
          testID="ring-sound"
          icon="call"
          title={t('settings.ring_on')}
          toggle={{ value: ringSound, onChange: (on) => updateSettings(store, { ringSound: on }) }}
        />
      </ListSection>
      {accounts.map((summary, index) => (
        <ListSection
          key={summary.id}
          title={accounts.length > 1 ? summary.display_name : t('settings.notifications')}
          footer={index === accounts.length - 1 ? t('mobile.accounts.notifications_hint') : undefined}
        >
          {accounts.length > 1 ? (
            <>
              <ListRow
                leading={<Avatar account={summary.id} id={summary.id} name={summary.display_name} media={summary.avatar} size={28} />}
                title={summary.display_name}
                subtitle={summary.id}
              />
              <Divider />
            </>
          ) : null}
          {pushReady && push.pushEnabled && accounts.length > 1 ? (
            <>
              <ListRow
                testID={`push-${summary.id}`}
                icon="bell"
                title={t('mobile.push.account')}
                subtitle={t('mobile.push.account_hint')}
                toggle={{ value: accountPushOn(push, summary.id), onChange: (on) => setAccountPush(store, summary.id, on) }}
              />
              <Divider />
            </>
          ) : null}
          {NOTIFICATION_MODES.map((mode, i) => (
            <Fragment key={mode.id}>
              {i > 0 ? <Divider inset={52} /> : null}
              <ListRow
                testID={`default-${summary.id}-${mode.id}`}
                icon={mode.id === 'AllMessages' ? 'bell' : mode.id === 'MentionsOnly' ? 'at' : 'bellOff'}
                title={t(mode.label)}
                subtitle={t(mode.hint)}
                selected={summary.default_notifications === mode.id}
                chevron={false}
                onPress={() => setDefaultNotificationMode(store, summary.id, mode.id)}
              />
            </Fragment>
          ))}
        </ListSection>
      ))}
    </SettingsPage>
  );
}
