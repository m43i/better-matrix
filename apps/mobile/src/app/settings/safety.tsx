// Safety (ADR 0011), for one account: where pictures and videos show at once, whether strangers'
// invitations and invitations' pictures show, the invitations kept out of sight (each opens its
// usual sheet), and the people the user ignores. Every choice is saved in the account's data, so
// it holds on all the user's sessions.

import type { ShowMedia } from '@better-matrix/matrix-native/types';
import { router } from 'expo-router';
import { Fragment } from 'react';

import { t, type Key } from '../../i18n';
import { routes } from '../../routes';
import { changeSafety, setIgnored } from '../../state/actions/safety';
import { useAppStore } from '../../state/react';
import { hiddenInvitations } from '../../state/safety';
import { Avatar } from '../../ui/Avatar';
import { Divider, ListRow, ListSection } from '../../ui/components';
import {
  AccountHeading,
  NoAccount,
  SettingsPage,
  useSettingsAccount,
} from '../../ui/settings/common';

/** The media choices, in the order the page offers them. */
const MEDIA: ReadonlyArray<{ id: ShowMedia; label: Key; hint: Key }> = [
  {
    id: 'Always',
    label: 'mobile.safety.media_always',
    hint: 'mobile.safety.media_always_hint',
  },
  {
    id: 'Private',
    label: 'mobile.safety.media_private',
    hint: 'mobile.safety.media_private_hint',
  },
  { id: 'Never', label: 'mobile.safety.media_never', hint: 'mobile.safety.media_never_hint' },
];

export default function SafetyScreen() {
  const store = useAppStore();
  const account = useSettingsAccount();
  const title = t('settings.safety');

  if (!account) return <NoAccount title={title} />;
  const id = account.summary.id;
  const { safety } = account;
  const hidden = hiddenInvitations(account.invitations, safety);

  // A failed save says why in a notice and puts the choice back.
  const change = (...args: Parameters<typeof changeSafety>) =>
    void changeSafety(...args).catch(() => {});

  return (
    <SettingsPage title={title} testID="safety-settings">
      <AccountHeading account={account} />
      <ListSection title={t('safety.media')} footer={t('mobile.safety.media_footer')}>
        {MEDIA.map((choice, i) => (
          <Fragment key={choice.id}>
            {i > 0 ? <Divider inset={52} /> : null}
            <ListRow
              testID={`safety-media-${choice.id}`}
              icon={choice.id === 'Always' ? 'image' : choice.id === 'Private' ? 'lock' : 'eyeOff'}
              title={t(choice.label)}
              subtitle={t(choice.hint)}
              selected={safety.media === choice.id}
              chevron={false}
              onPress={() => change(store, id, { type: 'Media', data: choice.id })}
            />
          </Fragment>
        ))}
      </ListSection>
      <ListSection
        title={t('mobile.safety.invitations')}
        footer={t('mobile.safety.hide_strangers_hint')}
      >
        <ListRow
          testID="safety-hide-strangers"
          icon="mail"
          title={t('mobile.safety.hide_strangers')}
          toggle={{
            value: safety.hide_stranger_invites,
            onChange: (on) => change(store, id, { type: 'HideStrangerInvites', data: on }),
          }}
        />
        <Divider inset={52} />
        <ListRow
          testID="safety-invite-pictures"
          icon="image"
          title={t('mobile.safety.invite_pictures')}
          toggle={{
            value: safety.invite_pictures,
            onChange: (on) => change(store, id, { type: 'InvitePictures', data: on }),
          }}
        />
      </ListSection>
      {hidden.length > 0 ? (
        <ListSection title={t('mobile.safety.hidden_invitations')}>
          {hidden.map((invite, i) => (
            <Fragment key={invite.room_id}>
              {i > 0 ? <Divider inset={52} /> : null}
              <ListRow
                testID={`safety-hidden-${invite.room_id}`}
                leading={
                  <Avatar
                    account={id}
                    id={invite.room_id}
                    name={invite.name}
                    media={safety.invite_pictures ? invite.avatar : null}
                    size={28}
                  />
                }
                title={invite.name}
                subtitle={
                  invite.inviter
                    ? t('mobile.safety.invited_by', { name: invite.inviter.name })
                    : undefined
                }
                onPress={() => router.push(routes.invitation(id, invite.room_id))}
              />
            </Fragment>
          ))}
        </ListSection>
      ) : null}
      <ListSection title={t('safety.ignored')} footer={t('mobile.safety.ignored_footer')}>
        {safety.ignored.length === 0 ? (
          <ListRow testID="safety-ignored-none" title={t('safety.ignored_none')} />
        ) : (
          safety.ignored.map((userId, i) => (
            <Fragment key={userId}>
              {i > 0 ? <Divider inset={52} /> : null}
              <ListRow
                testID={`safety-ignored-${userId}`}
                leading={<Avatar account={id} id={userId} name={userId} media={null} size={28} />}
                title={userId}
                value={t('safety.stop_ignoring')}
                chevron={false}
                onPress={() => void setIgnored(store, id, userId, false).catch(() => {})}
              />
            </Fragment>
          ))
        )}
      </ListSection>
    </SettingsPage>
  );
}
