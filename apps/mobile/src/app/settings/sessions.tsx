// Sessions: everywhere the account is signed in, this session first, each with its last
// activity and verification. A session's row offers renaming it and signing it out; the server
// may ask for the password first, and an OAuth account ends it on the server's page.

import type { Session } from '@better-matrix/matrix-native/types';
import { useFocusEffect } from 'expo-router';
import { Fragment, useCallback } from 'react';
import { ActionSheetIOS, Alert, Platform, StyleSheet, Text, View } from 'react-native';

import { locale, t } from '../../i18n';
import { endSession, listSessions, renameSession } from '../../state/actions/security';
import { useAppStore } from '../../state/react';
import { font } from '../../theme/palette';
import { openInBrowser } from '../../ui/accounts/browser';
import { usePrompt } from '../../ui/accounts/Prompt';
import { Divider, ListRow, ListSection, Touchable } from '../../ui/components';
import { Icon } from '../../ui/icons';
import {
  AccountHeading,
  NoAccount,
  SettingsPage,
  useSettingsAccount,
} from '../../ui/settings/common';
import { sessionDetails, sortSessions } from '../../ui/settings/sessions';
import { usePalette } from '../../ui/theme';

function formatDate(ms: number): string {
  return new Date(ms).toLocaleString(locale(), { dateStyle: 'medium', timeStyle: 'short' });
}

export default function SessionsScreen() {
  const store = useAppStore();
  const account = useSettingsAccount();
  const prompt = usePrompt();
  const id = account?.summary.id ?? null;

  useFocusEffect(
    useCallback(() => {
      if (id) listSessions(store, id);
    }, [store, id]),
  );

  if (!account || id === null) return <NoAccount title={t('settings.sessions')} />;

  const rename = async (session: Session) => {
    const name = await prompt.ask({
      title: t('sessions.rename_title'),
      confirm: t('common.save'),
      defaultValue: session.name ?? session.device_id,
      placeholder: t('sessions.name_placeholder'),
    });

    if (name !== null) renameSession(store, id, session.device_id, name);
  };

  const end = (session: Session) =>
    endSession(store, id, session.device_id, {
      askPassword: (error) =>
        prompt.ask({
          title: t('sessions.sign_out_title'),
          message: t('sessions.password_hint'),
          error,
          confirm: t('sessions.sign_out'),
          secure: true,
          destructive: true,
          placeholder: t('sign_in.password'),
        }),
      openUrl: (url) => openInBrowser(store, url),
    });

  const actions = (session: Session) => {
    const name = session.name ?? session.device_id;

    if (Platform.OS === 'ios') {
      const options = [
        t('sessions.rename'),
        ...(session.current ? [] : [t('sessions.sign_out')]),
        t('common.cancel'),
      ];

      ActionSheetIOS.showActionSheetWithOptions(
        {
          title: name,
          options,
          cancelButtonIndex: options.length - 1,
          destructiveButtonIndex: session.current ? undefined : 1,
        },
        (index) => {
          if (index === 0) void rename(session);
          else if (index === 1 && !session.current) void end(session);
        },
      );

      return;
    }

    Alert.alert(name, undefined, [
      { text: t('common.cancel'), style: 'cancel' },
      ...(session.current
        ? []
        : [{ text: t('sessions.sign_out'), onPress: () => void end(session) }]),
      { text: t('sessions.rename'), onPress: () => void rename(session) },
    ]);
  };

  const sessions = account.sessions === null ? null : sortSessions(account.sessions);

  return (
    <SettingsPage title={t('settings.sessions')} testID="sessions">
      <AccountHeading account={account} />
      <ListSection title={t('settings.sessions')} footer={t('sessions.hint')}>
        {sessions === null ? (
          <ListRow title={t('sessions.loading')} />
        ) : (
          sessions.map((session, index) => (
            <Fragment key={session.device_id}>
              {index > 0 ? <Divider inset={52} /> : null}
              <SessionRow session={session} onPress={() => actions(session)} />
            </Fragment>
          ))
        )}
      </ListSection>
      {prompt.element}
    </SettingsPage>
  );
}

function SessionRow({ session, onPress }: { session: Session; onPress: () => void }) {
  const palette = usePalette();

  return (
    <Touchable testID={`session-${session.device_id}`} onPress={onPress} accessibilityRole="button">
      <View style={styles.row}>
        <Icon
          name="laptop"
          size={22}
          color={session.current ? palette.accent : palette.textMuted}
        />
        <View style={styles.text}>
          <Text style={[font.bodyStrong, { color: palette.text }]} numberOfLines={1}>
            {session.name ?? session.device_id}
          </Text>
          <View style={styles.badges}>
            {session.current ? <Chip label={t('sessions.this')} color={palette.accent} /> : null}
            <Chip
              label={session.verified ? t('sessions.verified') : t('sessions.unverified')}
              color={session.verified ? palette.success : palette.warning}
            />
          </View>
          <Text style={[font.meta, { color: palette.textMuted }]} numberOfLines={2}>
            {sessionDetails(session, formatDate)}
          </Text>
        </View>
        <Icon name="more" size={18} color={palette.textMuted} />
      </View>
    </Touchable>
  );
}

function Chip({ label, color }: { label: string; color: string }) {
  return (
    <View style={[styles.chip, { backgroundColor: `${color}26` }]}>
      <Text style={[font.badge, { color }]}>{label}</Text>
    </View>
  );
}

const styles = StyleSheet.create({
  row: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 14,
    paddingHorizontal: 16,
    paddingVertical: 12,
  },
  text: { flex: 1, gap: 4 },
  badges: { flexDirection: 'row', gap: 6 },
  chip: { borderRadius: 10, paddingHorizontal: 8, paddingVertical: 2 },
});
