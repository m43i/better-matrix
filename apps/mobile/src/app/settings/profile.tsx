// Profile: the account's picture, display name and contact details. Where the server leaves
// contact details to an OAuth service's account page, they show read-only with a way there.

import * as ImagePicker from 'expo-image-picker';
import { useEffect, useState } from 'react';
import { StyleSheet, Text, View } from 'react-native';

import { pathOf } from '../../engine/boot';
import { t } from '../../i18n';
import { usePersonName } from '../../ui/displayNames';
import {
  editableName,
  listContactDetails,
  nameChanged,
  openAccountPage,
  saveDisplayName,
  setAvatar,
} from '../../state/actions/accounts';
import { addNotice } from '../../state/reducer';
import { useAppStore } from '../../state/react';
import { font } from '../../theme/palette';
import { openInBrowser } from '../../ui/accounts/browser';
import { usePrompt } from '../../ui/accounts/Prompt';
import { Avatar } from '../../ui/Avatar';
import { Button, Divider, ListRow, ListSection, TextField } from '../../ui/components';
import { NoAccount, SettingsPage, useSettingsAccount } from '../../ui/settings/common';
import { ContactSection } from '../../ui/settings/ContactSection';
import { usePalette } from '../../ui/theme';

export default function ProfileScreen() {
  const store = useAppStore();
  const palette = usePalette();
  const account = useSettingsAccount();
  const prompt = usePrompt();
  const summary = account?.summary;
  const id = summary?.id ?? null;
  const displayName = usePersonName(summary?.display_name ?? '');
  const [name, setName] = useState(summary ? editableName(summary.id, summary.display_name) : '');
  const [savingName, setSavingName] = useState(false);
  const [savingAvatar, setSavingAvatar] = useState(false);

  useEffect(() => {
    if (id) listContactDetails(store, id);
  }, [store, id]);

  if (!account || !summary) return <NoAccount title={t('settings.profile')} />;

  const notice = (cause: unknown) =>
    store.update((s) =>
      addNotice(s, cause instanceof Error ? cause.message : String(cause), summary.id),
    );

  const saveName = () => {
    if (savingName || !nameChanged(summary.display_name, name)) return;
    setSavingName(true);
    saveDisplayName(store, summary.id, name)
      .catch(notice)
      .finally(() => setSavingName(false));
  };

  const changeAvatar = (file: string | null) => {
    setSavingAvatar(true);
    setAvatar(store, summary.id, file)
      .catch(notice)
      .finally(() => setSavingAvatar(false));
  };

  const pickAvatar = async () => {
    const result = await ImagePicker.launchImageLibraryAsync({
      mediaTypes: 'images',
      allowsEditing: true,
      aspect: [1, 1],
      quality: 0.9,
    });

    const picked = result.canceled ? undefined : result.assets[0];

    if (picked) changeAvatar(pathOf(picked.uri));
  };

  const askPassword = (error: string | null) =>
    prompt.ask({
      title: t('reset.password_title'),
      message: t('profile.password_hint'),
      error,
      confirm: t('profile.add'),
      secure: true,
      placeholder: t('sign_in.password'),
    });

  const contacts = account.contacts;
  const changeable = contacts === null || contacts.changeable;
  const details = contacts?.details ?? null;

  // A kind the user can't add to shows only when something is linked.
  const kinds = (['Email', 'Phone'] as const).filter(
    (kind) => changeable || details?.some((d) => d.kind === kind),
  );

  return (
    <SettingsPage title={t('settings.profile')} testID="profile">
      <View style={styles.picture}>
        <Avatar
          account={summary.id}
          id={summary.id}
          name={displayName}
          media={summary.avatar}
          size={96}
        />
        <Text style={[font.header, { color: palette.text }]} numberOfLines={1}>
          {displayName}
        </Text>
        <Text style={[font.meta, { color: palette.textMuted }]} selectable>
          {summary.id}
        </Text>
      </View>
      <ListSection>
        <ListRow
          testID="avatar-upload"
          icon="library"
          title={savingAvatar ? t('profile.uploading') : t('profile.upload')}
          onPress={savingAvatar ? undefined : () => void pickAvatar()}
        />
        {summary.avatar ? (
          <>
            <Divider inset={52} />
            <ListRow
              testID="avatar-remove"
              icon="trash"
              title={t('profile.remove_picture')}
              danger
              chevron={false}
              onPress={savingAvatar ? undefined : () => changeAvatar(null)}
            />
          </>
        ) : null}
      </ListSection>
      <ListSection title={t('profile.name')} footer={t('profile.name_hint')}>
        <View style={styles.nameForm}>
          <View style={styles.nameField}>
            <TextField
              testID="profile-name"
              value={name}
              onChangeText={setName}
              placeholder={t('profile.name_placeholder')}
              returnKeyType="done"
              onSubmitEditing={saveName}
            />
          </View>
          <Button
            testID="profile-name-save"
            title={savingName ? t('profile.saving') : t('common.save')}
            busy={savingName}
            disabled={!nameChanged(summary.display_name, name)}
            onPress={saveName}
          />
        </View>
      </ListSection>
      {kinds.map((kind) => (
        <ContactSection
          key={kind}
          account={summary.id}
          kind={kind}
          details={details}
          changeable={changeable}
          askPassword={askPassword}
        />
      ))}
      {!changeable && summary.sign_in === 'OAuth' ? (
        <ListSection
          title={t('profile.account_page')}
          footer={t('profile.account_page_hint', { server: summary.homeserver })}
        >
          <ListRow
            testID="account-page"
            icon="external"
            title={t('profile.open_account_page')}
            onPress={() =>
              openAccountPage(store, summary.id, (url) => openInBrowser(store, url)).catch(notice)
            }
          />
        </ListSection>
      ) : null}
      {prompt.element}
    </SettingsPage>
  );
}

const styles = StyleSheet.create({
  picture: { alignItems: 'center', gap: 6, paddingTop: 24 },
  nameForm: { flexDirection: 'row', alignItems: 'center', gap: 8, padding: 12 },
  nameField: { flex: 1 },
});
