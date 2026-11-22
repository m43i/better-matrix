// The parts of a room's or space's settings screen. Each is shown only when the user has its
// right (`settingsParts`), and within a part each control only with its own right.

import type { RoomSetting, RoomSettings } from '@better-matrix/matrix-native/types';
import * as ImagePicker from 'expo-image-picker';
import { useEffect, useRef, useState } from 'react';
import { Alert, StyleSheet, View } from 'react-native';

import { pathOf } from '../../engine/boot';
import { t } from '../../i18n';
import {
  addressToAdd,
  allAddresses,
  directoryOffered,
  joinRulesForSettings,
  textSetting,
} from '../../state/admin';
import { Button, Divider, ListRow, ListSection, TextField } from '../components';
import { HISTORY_CHOICES, historyChoiceLabel } from './labels';
import { JoinRuleChoice } from './JoinRuleChoice';

/** What every part takes: the settings as shown, and how to change one. */
interface PartProps {
  settings: RoomSettings;
  change: (setting: RoomSetting) => Promise<void>;
}

/** Sends a change, ignoring its failure: the store told the user already. */
function send(change: PartProps['change'], setting: RoomSetting): void {
  change(setting).catch(() => {});
}

/** A text setting with its Save button, on while the text differs from the room's. */
function TextSetting({
  which,
  current,
  change,
  testID,
}: {
  which: 'Name' | 'Topic';
  current: string | null;
  change: PartProps['change'];
  testID: string;
}) {
  const [text, setText] = useState(current ?? '');
  const [saving, setSaving] = useState(false);
  // The field holds the user's text: a fresh value from the room replaces it only when they
  // haven't typed since it was filled, and never while a save is on its way (that save decides).
  const edited = useRef(false);
  const pending = useRef(false);
  const latest = useRef(current ?? '');

  useEffect(() => {
    latest.current = current ?? '';

    if (!edited.current && !pending.current) setText(current ?? '');
  }, [current]);

  const type = (typed: string) => {
    edited.current = true;
    setText(typed);
  };

  const setting = textSetting(which, current, text);

  const save = () => {
    if (setting === null || saving) return;
    edited.current = false;
    pending.current = true;
    setSaving(true);
    change(setting)
      .then(
        // Saved: the room's value now, unless the user typed again meanwhile.
        () => {
          if (!edited.current) setText(latest.current);
        },
        // Failed: the draft stays, still the user's, to try again; the notice says why.
        () => {
          edited.current = true;
        },
      )
      .finally(() => {
        pending.current = false;
        setSaving(false);
      });
  };

  return (
    <View style={styles.textRow}>
      <View style={styles.field}>
        <TextField
          testID={testID}
          value={text}
          onChangeText={type}
          multiline={which === 'Topic'}
          maxLength={which === 'Name' ? 255 : undefined}
          returnKeyType={which === 'Name' ? 'done' : 'default'}
          onSubmitEditing={which === 'Name' ? save : undefined}
        />
      </View>
      <Button
        testID={`${testID}-save`}
        title={t('common.save')}
        busy={saving}
        disabled={setting === null}
        onPress={save}
      />
    </View>
  );
}

/** Name, topic and picture. */
/**
 * The name, topic and picture. The text fields follow the room's confirmed settings, not the
 * changes on their way: a failed save then leaves the draft as typed, to try again.
 */
export function GeneralPart({
  settings,
  confirmed,
  change,
}: PartProps & { confirmed: RoomSettings }) {
  const { rights } = settings;
  const [savingPicture, setSavingPicture] = useState(false);

  const setPicture = (path: string | null) => {
    setSavingPicture(true);
    change({ type: 'Avatar', data: path })
      .catch(() => {})
      .finally(() => setSavingPicture(false));
  };

  const pick = async () => {
    const result = await ImagePicker.launchImageLibraryAsync({
      mediaTypes: 'images',
      allowsEditing: true,
      aspect: [1, 1],
      quality: 0.9,
    });

    const picked = result.canceled ? undefined : result.assets[0];

    if (picked) setPicture(pathOf(picked.uri));
  };

  return (
    <>
      {rights.name ? (
        <ListSection title={t('mobile.admin.name')}>
          <TextSetting
            which="Name"
            current={confirmed.name}
            change={change}
            testID="settings-name"
          />
        </ListSection>
      ) : null}
      {rights.topic ? (
        <ListSection title={t('mobile.admin.topic')}>
          <TextSetting
            which="Topic"
            current={confirmed.topic}
            change={change}
            testID="settings-topic"
          />
        </ListSection>
      ) : null}
      {rights.avatar ? (
        <ListSection>
          <ListRow
            testID="settings-picture"
            icon="library"
            title={savingPicture ? t('mobile.admin.saving') : t('mobile.admin.picture_change')}
            onPress={savingPicture ? undefined : () => void pick()}
          />
          {settings.avatar ? (
            <>
              <Divider inset={52} />
              <ListRow
                testID="settings-picture-remove"
                icon="trash"
                title={t('mobile.admin.picture_remove')}
                danger
                chevron={false}
                onPress={savingPicture ? undefined : () => setPicture(null)}
              />
            </>
          ) : null}
        </ListSection>
      ) : null}
    </>
  );
}

/** Who may join, guests, who may read history and the directory listing. */
export function AccessPart({ settings, change }: PartProps) {
  const { rights } = settings;
  const server = settings.server;

  return (
    <>
      {rights.join_rule ? (
        <JoinRuleChoice
          testID="settings-join"
          rules={joinRulesForSettings(settings)}
          value={settings.join_rule}
          // Sent even when it shows as the rule in force: one picked before may still be on its
          // way, and the engine skips a rule in force when the change runs.
          onChange={(rule) => send(change, { type: 'JoinRule', data: rule })}
        />
      ) : null}
      {rights.guest_access || directoryOffered(settings) ? (
        <ListSection
          footer={
            directoryOffered(settings)
              ? t('mobile.admin.directory_hint', { server })
              : t('mobile.admin.guests_hint')
          }
        >
          {rights.guest_access ? (
            <ListRow
              testID="settings-guests"
              title={t('mobile.admin.guests')}
              toggle={{
                value: settings.guest_access,
                onChange: (on) => send(change, { type: 'GuestAccess', data: on }),
              }}
            />
          ) : null}
          {rights.guest_access && directoryOffered(settings) ? <Divider inset={16} /> : null}
          {directoryOffered(settings) ? (
            <ListRow
              testID="settings-directory"
              title={t('mobile.admin.directory')}
              toggle={{
                value: settings.published === true,
                onChange: (on) => send(change, { type: 'Published', data: on }),
              }}
            />
          ) : null}
        </ListSection>
      ) : null}
      {rights.history ? (
        <ListSection title={t('mobile.admin.history')}>
          {HISTORY_CHOICES.map((history, index) => (
            <View key={history}>
              {index > 0 ? <Divider inset={16} /> : null}
              <ListRow
                testID={`settings-history-${history}`}
                title={historyChoiceLabel(history)}
                selected={settings.history === history}
                chevron={false}
                onPress={
                  settings.history === history
                    ? undefined
                    : () => send(change, { type: 'History', data: history })
                }
              />
            </View>
          ))}
        </ListSection>
      ) : null}
    </>
  );
}

/** The room's addresses: the main one first, each made main or removed; adding one. */
export function AddressesPart({ settings, change }: PartProps) {
  const [typed, setTyped] = useState('');
  const addresses = allAddresses(settings);
  const toAdd = addressToAdd(settings, typed);

  const act = (address: string) => {
    const main = address === settings.main_address;

    Alert.alert(address, undefined, [
      { text: t('common.cancel'), style: 'cancel' },
      ...(main
        ? []
        : [
            {
              text: t('mobile.admin.make_main'),
              onPress: () => send(change, { type: 'MainAddress', data: address }),
            },
          ]),
      {
        text: t('mobile.admin.remove_address'),
        style: 'destructive' as const,
        onPress: () => send(change, { type: 'RemoveAddress', data: address }),
      },
    ]);
  };

  const add = () => {
    if (toAdd === null) return;
    setTyped('');
    send(change, { type: 'AddAddress', data: toAdd });
  };

  return (
    <ListSection
      title={t('mobile.admin.addresses')}
      footer={t('mobile.admin.addresses_hint', { server: settings.server })}
    >
      {addresses.length === 0 ? (
        <ListRow title={t('mobile.admin.no_addresses')} />
      ) : (
        addresses.map((address, index) => (
          <View key={address}>
            {index > 0 ? <Divider inset={52} /> : null}
            <ListRow
              testID={`settings-address-${address}`}
              icon="hash"
              title={address}
              subtitle={
                address === settings.main_address ? t('mobile.admin.main_address') : undefined
              }
              chevron={false}
              onPress={() => act(address)}
            />
          </View>
        ))
      )}
      <View style={styles.textRow}>
        <View style={styles.field}>
          <TextField
            testID="settings-address-new"
            value={typed}
            onChangeText={setTyped}
            placeholder={t('mobile.admin.add_address_placeholder')}
            autoCapitalize="none"
            autoCorrect={false}
            returnKeyType="done"
            onSubmitEditing={add}
          />
        </View>
        <Button
          testID="settings-address-add"
          title={t('mobile.admin.add')}
          disabled={toAdd === null}
          onPress={add}
        />
      </View>
    </ListSection>
  );
}

/** Turning on encryption, once, after the user confirmed it. */
export function SecurityPart({ change }: PartProps) {
  const ask = () =>
    Alert.alert(t('mobile.admin.encryption_title'), t('mobile.admin.encryption_body'), [
      { text: t('common.cancel'), style: 'cancel' },
      {
        text: t('mobile.admin.encryption_confirm'),
        style: 'destructive',
        onPress: () => send(change, { type: 'Encrypt' }),
      },
    ]);

  return (
    <ListSection title={t('mobile.admin.security')} footer={t('mobile.admin.encryption_hint')}>
      <ListRow
        testID="settings-encrypt"
        icon="lock"
        title={t('mobile.admin.encryption_on')}
        chevron={false}
        onPress={ask}
      />
    </ListSection>
  );
}

/** The room version, and upgrading it when the server has a newer one. */
export function AdvancedPart({
  settings,
  onUpgrade,
}: {
  settings: RoomSettings;
  onUpgrade: (version: string) => void;
}) {
  const target = settings.upgrade_to;

  const ask = (version: string) =>
    Alert.alert(t('mobile.admin.upgrade_title'), t('mobile.admin.upgrade_body', { version }), [
      { text: t('common.cancel'), style: 'cancel' },
      {
        text: t('mobile.admin.upgrade_confirm'),
        style: 'destructive',
        onPress: () => onUpgrade(version),
      },
    ]);

  return (
    <ListSection
      title={t('mobile.admin.advanced')}
      footer={target ? t('mobile.admin.upgrade_hint') : undefined}
    >
      <ListRow icon="info" title={t('mobile.admin.version')} value={settings.version} />
      {target ? (
        <>
          <Divider inset={52} />
          <ListRow
            testID="settings-upgrade"
            icon="upgrade"
            title={t('mobile.admin.upgrade', { version: target })}
            chevron={false}
            onPress={() => ask(target)}
          />
        </>
      ) : null}
    </ListSection>
  );
}

/** Opens one of the settings' own pages (permissions, a space's rooms). */
export function LinkPart({
  title,
  icon,
  onPress,
  testID,
}: {
  title: string;
  icon: 'shield' | 'space';
  onPress: () => void;
  testID: string;
}) {
  return (
    <ListSection>
      <ListRow testID={testID} icon={icon} title={title} onPress={onPress} />
    </ListSection>
  );
}

const styles = StyleSheet.create({
  textRow: { flexDirection: 'row', alignItems: 'center', gap: 8, padding: 12 },
  field: { flex: 1 },
});
