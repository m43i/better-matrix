// The form that makes a room or space: its name and topic, the space a room goes in, who may
// join, and for a public one its address and directory listing; a room's encryption (on unless
// it is public, until the user sets it). The new-room and new-space screens wrap it.

import type { AccountId, JoinRule, SpaceSummary } from '@better-matrix/matrix-native/types';
import { useState } from 'react';
import { View } from 'react-native';

import { t } from '../../i18n';
import {
  addressInvalid,
  addressLocalPart,
  createRoomCommand,
  EMPTY_NEW_ROOM,
  encryptsByDefault,
  joinRulesOffered,
  type NewRoomDraft,
  serverOf,
} from '../../state/admin';
import type { CommandOf } from '../../state/people';
import { Avatar } from '../Avatar';
import { Divider, ListRow, ListSection, TextField } from '../components';
import { RoomForm } from '../people/RoomForm';
import { JoinRuleChoice } from './JoinRuleChoice';

export function NewRoomForm({
  account,
  space,
  spaces,
  initial,
  onCreate,
  testID,
}: {
  account: AccountId;
  /** Making a space rather than a room. */
  space: boolean;
  /** The spaces a room may go in (none for a space). */
  spaces: SpaceSummary[];
  initial: { parent: string | null; joinRule: JoinRule | null };
  /** Makes it; rejects when the engine couldn't. */
  onCreate: (command: CommandOf<'CreateRoom'>) => Promise<void>;
  testID: string;
}) {
  const [draft, setDraft] = useState<NewRoomDraft>(() => ({
    ...EMPTY_NEW_ROOM,
    parent: initial.parent,
    joinRule: initial.joinRule ?? 'Invite',
  }));

  const [busy, setBusy] = useState(false);
  const change = (part: Partial<NewRoomDraft>) => setDraft((d) => ({ ...d, ...part }));

  const command = createRoomCommand(account, draft, space);
  const rules = joinRulesOffered(space, draft.parent !== null);
  const server = serverOf(account);
  const local = addressLocalPart(draft.address);
  const chosen = spaces.find((s) => s.room_id === draft.parent);

  const submit = () => {
    if (command === null || busy) return;
    setBusy(true);
    onCreate(command).catch(() => setBusy(false));
  };

  const setParent = (parent: string | null) =>
    // Space members needs a space: without one the room falls back to private.
    change({
      parent,
      joinRule: parent === null && draft.joinRule === 'Restricted' ? 'Invite' : draft.joinRule,
    });

  return (
    <RoomForm
      testID={testID}
      title={space ? t('create.space_title') : t('create.room_title')}
      confirm={t('create.create')}
      valid={command !== null}
      busy={busy}
      onSubmit={submit}
    >
      <TextField
        testID={`${testID}-name`}
        label={t('create.name')}
        value={draft.name}
        onChangeText={(name) => change({ name })}
        autoFocus
        returnKeyType="next"
        maxLength={255}
      />
      <TextField
        testID={`${testID}-topic`}
        label={t('create.topic')}
        value={draft.topic}
        onChangeText={(topic) => change({ topic })}
        returnKeyType="done"
      />
      {!space && spaces.length > 0 ? (
        <ListSection
          title={t('mobile.people.space')}
          footer={chosen ? t('create.room_in', { space: chosen.name }) : undefined}
        >
          <ListRow
            testID={`${testID}-space-none`}
            title={t('mobile.people.no_space')}
            icon="home"
            selected={draft.parent === null}
            chevron={false}
            onPress={() => setParent(null)}
          />
          {spaces.map((s) => (
            <View key={s.room_id}>
              <Divider inset={52} />
              <ListRow
                testID={`${testID}-space-${s.room_id}`}
                title={s.name}
                leading={
                  <Avatar
                    account={account}
                    id={s.room_id}
                    name={s.name}
                    media={s.avatar}
                    size={28}
                  />
                }
                selected={draft.parent === s.room_id}
                chevron={false}
                onPress={() => setParent(s.room_id)}
              />
            </View>
          ))}
        </ListSection>
      ) : null}
      <JoinRuleChoice
        testID={`${testID}-join`}
        rules={rules}
        value={draft.joinRule}
        onChange={(joinRule) => change({ joinRule })}
      />
      {draft.joinRule === 'Public' ? (
        <>
          <TextField
            testID={`${testID}-address`}
            label={t('mobile.admin.address')}
            value={draft.address}
            onChangeText={(address) => change({ address })}
            placeholder={t('mobile.admin.address_placeholder')}
            autoCapitalize="none"
            autoCorrect={false}
            error={addressInvalid(draft.address) ? t('mobile.admin.address_invalid') : null}
            hint={
              local === null
                ? t('mobile.admin.address_hint', { server })
                : t('mobile.admin.address_preview', { address: local, server })
            }
          />
          <ListSection footer={t('mobile.admin.directory_hint', { server })}>
            <ListRow
              testID={`${testID}-directory`}
              title={t('mobile.admin.directory')}
              toggle={{ value: draft.published, onChange: (published) => change({ published }) }}
            />
          </ListSection>
        </>
      ) : null}
      {space ? null : (
        <ListSection title={t('mobile.admin.security')} footer={t('mobile.admin.encrypt_hint')}>
          <ListRow
            testID={`${testID}-encrypt`}
            title={t('mobile.admin.encrypt')}
            icon="lock"
            toggle={{
              value: encryptsByDefault(draft),
              onChange: (encrypted) => change({ encrypted }),
            }}
          />
        </ListSection>
      )}
    </RoomForm>
  );
}
