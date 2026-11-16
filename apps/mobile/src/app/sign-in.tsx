// Signing in: the desktop's sign-in card laid out for a phone, in two steps (homeserver, then the
// browser or a password). Full screen when signed out; a modal when adding an account; `resume`
// signs in again as an account whose session ended.

import type { AccountId, Homeserver } from '@better-matrix/matrix-native/types';
import { router, useLocalSearchParams } from 'expo-router';
import { useEffect, useRef, useState, useSyncExternalStore } from 'react';
import { StyleSheet, Text, View } from 'react-native';
import { KeyboardAwareScrollView } from 'react-native-keyboard-controller';
import { SafeAreaView } from 'react-native-safe-area-context';

import { t } from '../i18n';
import { removeEnded, serverHost, serverName, SignInFlow, type SignInStep } from '../state/actions/accounts';
import { shallow, useAppStore, useStore } from '../state/react';
import { font } from '../theme/palette';
import { authBrowser } from '../ui/accounts/browser';
import { Logo } from '../ui/accounts/Logo';
import { Button, ListSection, TextField } from '../ui/components';
import { Icon } from '../ui/icons';
import { usePalette } from '../ui/theme';

export default function SignInScreen() {
  const store = useAppStore();
  const palette = usePalette();
  const { resume } = useLocalSearchParams<{ resume?: AccountId }>();
  const adding = useStore((s) => s.accounts.length > 0);
  const [flow] = useState(() => new SignInFlow(store, authBrowser));
  const { step, username: filled } = useSyncExternalStore(flow.subscribe, flow.snapshot, flow.snapshot);
  const [homeserver, setHomeserver] = useState(resume ?? 'matrix.org');
  const [username, setUsername] = useState('');
  const [password, setPassword] = useState('');

  // A full Matrix ID fills in the username.
  useEffect(() => {
    if (filled) setUsername(filled);
  }, [filled]);

  // Signing in again as an account starts from its ID.
  const started = useRef(false);
  useEffect(() => {
    if (resume && !started.current) {
      started.current = true;
      flow.submitServer(resume);
    }
  }, [flow, resume]);

  // Leaving stops what the sign-in waits on.
  useEffect(() => () => flow.cancelPending(), [flow]);

  // Signed in: the first account opens Home by itself (the root guards on accounts); an added
  // one closes this modal.
  const done = step.type === 'done';
  useEffect(() => {
    if (done && router.canGoBack()) router.back();
  }, [done]);

  const close = () => {
    flow.cancelPending();
    router.back();
  };

  return (
    <SafeAreaView style={[styles.screen, { backgroundColor: palette.bg }]}>
      {adding ? (
        <View style={styles.topBar}>
          <Button testID="sign-in-close" title={t('common.cancel')} kind="plain" onPress={close} />
        </View>
      ) : null}
      <KeyboardAwareScrollView
        contentContainerStyle={styles.content}
        keyboardShouldPersistTaps="handled"
        bottomOffset={24}
      >
        <View style={styles.brand}>
          <Logo size={40} color={palette.accent} />
          <Text style={[font.workspace, { color: palette.text, fontSize: 22 }]}>{t('app.name')}</Text>
        </View>
        {step.type === 'server' || step.type === 'discovering' ? (
          <ServerStep
            step={step}
            adding={adding}
            homeserver={homeserver}
            onChange={setHomeserver}
            onSubmit={(text) => flow.submitServer(text)}
            onResume={(account) => {
              setHomeserver(account);
              flow.submitServer(account);
            }}
          />
        ) : step.type === 'browser' ? (
          <>
            <ServerRow server={step.server} onChange={() => flow.changeServer()} />
            <BrowserStep
              server={step.server}
              waiting={step.request !== null}
              opened={step.url !== null}
              onRetry={() => flow.retryBrowser()}
              onReopen={() => flow.reopenBrowser()}
              onPassword={() => flow.usePassword()}
            />
          </>
        ) : step.type === 'password' ? (
          <>
            <ServerRow server={step.server} onChange={() => flow.changeServer()} />
            <View style={styles.step}>
              <Text style={[font.title, { color: palette.text }]}>{t('sign_in.password_title')}</Text>
              <TextField
                testID="username"
                label={t('sign_in.username')}
                value={username}
                onChangeText={setUsername}
                placeholder="alice"
                autoCapitalize="none"
                autoCorrect={false}
                autoComplete="username"
                textContentType="username"
                returnKeyType="next"
              />
              <TextField
                testID="password"
                label={t('sign_in.password')}
                value={password}
                onChangeText={setPassword}
                placeholder={t('sign_in.password')}
                secureTextEntry
                autoComplete="current-password"
                textContentType="password"
                returnKeyType="go"
                onSubmitEditing={() => flow.submitPassword(username, password)}
              />
              <Button
                testID="sign-in"
                title={step.submitting !== null ? t('sign_in.signing_in') : t('sign_in.sign_in')}
                busy={step.submitting !== null}
                onPress={() => flow.submitPassword(username, password)}
              />
              {step.server.methods.oauth ? (
                <Button testID="use-browser" title={t('sign_in.use_browser')} kind="plain" onPress={() => flow.retryBrowser()} />
              ) : null}
            </View>
          </>
        ) : (
          <View style={styles.step}>
            <Text style={[font.body, { color: palette.textMuted }]}>{t('sign_in.signing_in')}</Text>
          </View>
        )}
        <StepError step={step} />
      </KeyboardAwareScrollView>
    </SafeAreaView>
  );
}

function StepError({ step }: { step: SignInStep }) {
  const palette = usePalette();
  const error = step.type === 'server' || step.type === 'browser' || step.type === 'password' ? step.error : null;
  if (!error) return null;
  return (
    <Text testID="sign-in-error" style={[font.body, { color: palette.danger }]}>
      {error}
    </Text>
  );
}

function ServerStep({
  step,
  adding,
  homeserver,
  onChange,
  onSubmit,
  onResume,
}: {
  step: Extract<SignInStep, { type: 'server' | 'discovering' }>;
  adding: boolean;
  homeserver: string;
  onChange: (text: string) => void;
  onSubmit: (text: string) => void;
  onResume: (account: AccountId) => void;
}) {
  const store = useAppStore();
  const palette = usePalette();
  const ended = useStore((s) => s.ended, shallow);
  const discovering = step.type === 'discovering';
  return (
    <View style={styles.step}>
      <Text style={[font.title, { color: palette.text }]}>{adding ? t('sign_in.add_title') : t('sign_in.title')}</Text>
      {ended.length > 0 ? (
        <View style={styles.ended}>
          <ListSection title={t('sign_in.signed_out_heading')}>
            {ended.map(({ account, kept }) => (
              <EndedRow
                key={account}
                account={account}
                kept={kept}
                onResume={() => onResume(account)}
                onRemove={() => removeEnded(store, account)}
              />
            ))}
          </ListSection>
        </View>
      ) : null}
      <TextField
        testID="homeserver"
        label={t('sign_in.homeserver')}
        hint={t('sign_in.homeserver_hint')}
        value={homeserver}
        onChangeText={onChange}
        placeholder="matrix.org"
        autoCapitalize="none"
        autoCorrect={false}
        keyboardType="url"
        returnKeyType="go"
        editable={!discovering}
        onSubmitEditing={() => onSubmit(homeserver)}
      />
      <Button
        testID="continue"
        title={discovering ? t('sign_in.looking_up', { server: step.server }) : t('common.continue')}
        busy={discovering}
        onPress={() => onSubmit(homeserver)}
      />
    </View>
  );
}

/** An account signed out here: sign in as it again, or forget it. */
function EndedRow({
  account,
  kept,
  onResume,
  onRemove,
}: {
  account: AccountId;
  kept: boolean;
  onResume: () => void;
  onRemove: () => void;
}) {
  const palette = usePalette();
  return (
    <View style={styles.endedRow}>
      <Icon name="user" size={22} color={palette.textMuted} />
      <View style={styles.serverNames}>
        <Text style={[font.body, { color: palette.text }]} numberOfLines={1}>
          {account}
        </Text>
        {kept ? (
          <Text style={[font.meta, { color: palette.textMuted }]} numberOfLines={1}>
            {t('accounts.signed_out')}
          </Text>
        ) : null}
      </View>
      <Button testID={`resume-${account}`} title={t('session_ended.sign_in')} kind="plain" onPress={onResume} />
      <Button testID={`forget-${account}`} title={t('common.remove')} kind="plain" onPress={onRemove} />
    </View>
  );
}

/** The resolved homeserver, where it lives if that differs, and a way back to step 1. */
function ServerRow({ server, onChange }: { server: Homeserver; onChange: () => void }) {
  const palette = usePalette();
  const name = serverName(server);
  const host = serverHost(server);
  return (
    <View style={[styles.serverRow, { borderColor: palette.divider, backgroundColor: palette.surface }]}>
      <Icon name="globe" size={20} color={palette.textMuted} />
      <View style={styles.serverNames}>
        <Text testID="server-name" style={[font.bodyStrong, { color: palette.text }]} numberOfLines={1}>
          {name}
        </Text>
        {host !== name ? (
          <Text style={[font.meta, { color: palette.textMuted }]} numberOfLines={1}>
            {t('sign_in.via', { host })}
          </Text>
        ) : null}
      </View>
      <Button testID="change-server" title={t('common.change')} kind="plain" onPress={onChange} />
    </View>
  );
}

function BrowserStep({
  server,
  waiting,
  opened,
  onRetry,
  onReopen,
  onPassword,
}: {
  server: Homeserver;
  waiting: boolean;
  opened: boolean;
  onRetry: () => void;
  onReopen: () => void;
  onPassword: () => void;
}) {
  const palette = usePalette();
  return (
    <View style={styles.step}>
      <Text style={[font.title, { color: palette.text }]}>{t('sign_in.browser_title')}</Text>
      <Text style={[font.body, { color: palette.textMuted }]}>
        {waiting ? t('mobile.accounts.browser_waiting', { server: serverName(server) }) : t('sign_in.browser_failed')}
      </Text>
      {!waiting ? (
        <Button testID="retry-browser" title={t('sign_in.try_again')} onPress={onRetry} />
      ) : opened ? (
        <Button testID="reopen-browser" title={t('sign_in.reopen')} kind="secondary" onPress={onReopen} />
      ) : (
        <Button title={t('sign_in.browser_title')} busy onPress={onReopen} />
      )}
      {server.methods.password ? (
        <Button testID="use-password" title={t('sign_in.use_password')} kind="plain" onPress={onPassword} />
      ) : null}
    </View>
  );
}

const styles = StyleSheet.create({
  screen: { flex: 1 },
  topBar: { flexDirection: 'row', justifyContent: 'flex-start', paddingHorizontal: 8 },
  content: { flexGrow: 1, justifyContent: 'center', padding: 24, gap: 24, maxWidth: 480, width: '100%', alignSelf: 'center' },
  brand: { flexDirection: 'row', alignItems: 'center', gap: 12 },
  step: { gap: 16 },
  ended: { marginHorizontal: -16, marginTop: -22 },
  serverRow: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 12,
    paddingLeft: 14,
    borderRadius: 12,
    borderWidth: StyleSheet.hairlineWidth,
  },
  serverNames: { flex: 1, gap: 2 },
  endedRow: { flexDirection: 'row', alignItems: 'center', gap: 8, paddingLeft: 16, minHeight: 52 },
});
