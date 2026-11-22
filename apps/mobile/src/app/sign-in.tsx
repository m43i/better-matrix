// Signing in: the desktop's sign-in card laid out for a phone, in two steps (homeserver, then the
// browser or a password). Full screen, with a close button when adding an account; `resume`
// signs in again as an account whose session ended. Two other ways in: scanning a signed-in
// device's QR code (`qr`, offered before any server is chosen) or showing one on a server that
// allows it, and creating an account (`create` with a `server`; OAuth servers on their page,
// password servers with the registration form and what the server asks on the way).

import type {
  AccountId,
  Answer,
  Homeserver,
  RegistrationStep,
} from '@better-matrix/matrix-native/types';
import { router, useLocalSearchParams } from 'expo-router';
import { useEffect, useRef, useState, useSyncExternalStore } from 'react';
import { StyleSheet, Text, View } from 'react-native';
import { KeyboardAwareScrollView } from 'react-native-keyboard-controller';
import { SafeAreaView } from 'react-native-safe-area-context';

import { t } from '../i18n';
import {
  otherWays,
  removeEnded,
  serverHost,
  serverName,
  SignInFlow,
  type SignInStep,
} from '../state/actions/accounts';
import { QrLoginFlow, signInWithQr } from '../state/actions/qrLogin';
import { shallow, useAppStore, useStore } from '../state/react';
import { font } from '../theme/palette';
import { authBrowser, openInBrowser } from '../ui/accounts/browser';
import { Logo } from '../ui/accounts/Logo';
import { Button, IconButton, ListRow, ListSection, TextField } from '../ui/components';
import { Icon } from '../ui/icons';
import { QrLoginPanel } from '../ui/qr/QrLoginPanel';
import { usePalette } from '../ui/theme';

export default function SignInScreen() {
  const store = useAppStore();
  const palette = usePalette();

  const params = useLocalSearchParams<{
    resume?: AccountId;
    qr?: string;
    server?: string;
    create?: string;
  }>();

  const { resume } = params;
  const adding = useStore((s) => s.accounts.length > 0);

  const [flow] = useState(
    () => new SignInFlow(store, authBrowser, (url) => openInBrowser(store, url)),
  );

  // Signing in with a QR code instead, once chosen: through the server of step 2, if any.
  const [qr, setQr] = useState<QrLoginFlow | null>(null);

  const startQr = (server: Homeserver | null) => {
    flow.cancelPending();
    qr?.cancel();
    const next = new QrLoginFlow(store, signInWithQr(server), (url) => openInBrowser(store, url));
    next.scan();
    setQr(next);
  };

  const leaveQr = () => {
    qr?.cancel();
    setQr(null);
  };

  const {
    step,
    username: filled,
    resume: resuming,
  } = useSyncExternalStore(flow.subscribe, flow.snapshot, flow.snapshot);

  const [homeserver, setHomeserver] = useState(resume ?? 'matrix.org');
  const [username, setUsername] = useState(filled);
  const [lastFilled, setLastFilled] = useState(filled);

  if (lastFilled !== filled) {
    setLastFilled(filled);

    if (filled) setUsername(filled);
  }

  const [password, setPassword] = useState('');

  // Signing in again as an account starts from its ID; creating one, from its server; the QR
  // code, with the camera.
  const started = useRef(false);
  useEffect(() => {
    if (started.current) return;
    started.current = true;

    if (resume) flow.submitServer(resume);
    else if (params.server) {
      setHomeserver(params.server);
      flow.submitServer(params.server, params.create === '1');
    } else if (params.qr === '1') startQr(null);
    // Only the parameters the page opened with count.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Leaving stops what the sign-in waits on.
  useEffect(() => () => flow.cancelPending(), [flow]);
  useEffect(() => () => qr?.cancel(), [qr]);

  const qrView = useSyncExternalStore(
    qr?.subscribe ?? noSubscription,
    qr?.snapshot ?? noView,
    qr?.snapshot ?? noView,
  );

  // Signed in, the command finished (for QR, not just its last step: leaving cancels it): the
  // first account opens Home by itself (the root guards on accounts); an added one closes this
  // page.
  const done = step.type === 'done' || qrView?.type === 'done';
  useEffect(() => {
    if (done && router.canGoBack()) router.back();
  }, [done]);

  const close = () => {
    flow.cancelPending();
    qr?.cancel();
    router.back();
  };

  const serverOf = (current: SignInStep): Homeserver | null =>
    current.type === 'browser' || current.type === 'password' || current.type === 'register'
      ? current.server
      : null;

  return (
    <SafeAreaView style={[styles.screen, { backgroundColor: palette.bg }]}>
      {adding ? (
        <View style={styles.topBar}>
          <IconButton
            testID="sign-in-close"
            icon="close"
            label={t('common.close')}
            onPress={close}
          />
        </View>
      ) : null}
      <KeyboardAwareScrollView
        contentContainerStyle={styles.content}
        keyboardShouldPersistTaps="handled"
        bottomOffset={24}
      >
        <View style={styles.brand}>
          <Logo size={40} color={palette.accent} />
          <Text style={[font.workspace, { color: palette.text, fontSize: 22 }]}>
            {t('app.name')}
          </Text>
        </View>
        {qr !== null ? (
          <QrStep
            flow={qr}
            scanning={qrView?.type === 'scanning'}
            showing={qrView?.type === 'show'}
            onRetry={() => startQr(serverOf(step))}
            onBack={leaveQr}
          />
        ) : step.type === 'server' || step.type === 'discovering' ? (
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
            onQr={resuming === null ? () => startQr(null) : null}
          />
        ) : step.type === 'register' ? (
          <>
            <ServerRow server={step.server} onChange={() => flow.changeServer()} />
            <RegisterStep
              step={step}
              username={username}
              onUsername={setUsername}
              onSubmit={(name, secret, confirmation) =>
                flow.submitRegistration(name, secret, confirmation)
              }
              onAnswer={(answer) => flow.answerRegistration(answer)}
              onReopen={() => flow.reopenRegistrationPage()}
              onSignIn={() => flow.usePassword()}
            />
          </>
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
            <OtherWays
              server={step.server}
              resume={resuming}
              onCreate={() => flow.createAccount()}
              onQr={() => startQr(step.server)}
            />
          </>
        ) : step.type === 'password' ? (
          <>
            <ServerRow server={step.server} onChange={() => flow.changeServer()} />
            <View style={styles.step}>
              <Text style={[font.title, { color: palette.text }]}>
                {t('sign_in.password_title')}
              </Text>
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
                <Button
                  testID="use-browser"
                  title={t('sign_in.use_browser')}
                  kind="plain"
                  onPress={() => flow.retryBrowser()}
                />
              ) : null}
            </View>
            <OtherWays
              server={step.server}
              resume={resuming}
              onCreate={() => flow.createAccount()}
              onQr={() => startQr(step.server)}
            />
          </>
        ) : (
          <View style={styles.step}>
            <Text style={[font.body, { color: palette.textMuted }]}>{t('sign_in.signing_in')}</Text>
          </View>
        )}
        {qr === null ? <StepError step={step} /> : null}
      </KeyboardAwareScrollView>
    </SafeAreaView>
  );
}

/** A QR flow not started yet subscribes to nothing. */
const noSubscription = () => () => undefined;

const noView = () => null;

function StepError({ step }: { step: SignInStep }) {
  const palette = usePalette();

  const error =
    step.type === 'server' ||
    step.type === 'browser' ||
    step.type === 'password' ||
    step.type === 'register'
      ? step.error
      : null;

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
  onQr,
}: {
  step: Extract<SignInStep, { type: 'server' | 'discovering' }>;
  adding: boolean;
  homeserver: string;
  onChange: (text: string) => void;
  onSubmit: (text: string) => void;
  onResume: (account: AccountId) => void;
  /** `null` while signing in again as a suspended account. */
  onQr: (() => void) | null;
}) {
  const store = useAppStore();
  const palette = usePalette();
  const ended = useStore((s) => s.ended, shallow);
  const discovering = step.type === 'discovering';

  return (
    <View style={styles.step}>
      <Text style={[font.title, { color: palette.text }]}>
        {adding ? t('sign_in.add_title') : t('sign_in.title')}
      </Text>
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
        title={
          discovering ? t('sign_in.looking_up', { server: step.server }) : t('common.continue')
        }
        busy={discovering}
        onPress={() => onSubmit(homeserver)}
      />
      {onQr ? (
        <Button
          testID="sign-in-qr"
          title={t('mobile.accounts.sign_in_qr')}
          icon="qrScan"
          kind="secondary"
          disabled={discovering}
          onPress={onQr}
        />
      ) : null}
    </View>
  );
}

/** Signing in by QR code: the camera or this device's code, then what the exchange asks. */
function QrStep({
  flow,
  scanning,
  showing,
  onRetry,
  onBack,
}: {
  flow: QrLoginFlow;
  scanning: boolean;
  showing: boolean;
  onRetry: () => void;
  onBack: () => void;
}) {
  const palette = usePalette();

  return (
    <View style={styles.step}>
      <Text style={[font.title, { color: palette.text }]}>{t('mobile.accounts.sign_in_qr')}</Text>
      <QrLoginPanel
        flow={flow}
        scanHint={t('mobile.accounts.sign_in_qr_scan_hint')}
        showHint={t('mobile.accounts.sign_in_qr_show_hint')}
        doneText={t('sign_in.signing_in')}
        onRetry={onRetry}
      />
      {scanning && flow.canShow ? (
        <Button
          testID="sign-in-qr-show"
          title={t('mobile.accounts.sign_in_qr_show')}
          kind="plain"
          onPress={() => flow.show()}
        />
      ) : null}
      {showing ? (
        <Button
          testID="sign-in-qr-scan"
          title={t('mobile.accounts.sign_in_qr_scan_instead')}
          kind="plain"
          onPress={() => flow.scan()}
        />
      ) : null}
      <Button
        testID="sign-in-qr-back"
        title={t('mobile.accounts.back')}
        kind="plain"
        onPress={onBack}
      />
    </View>
  );
}

/** Step 2's other ways in (`otherWays`): creating an account, and a QR code. */
function OtherWays({
  server,
  resume,
  onCreate,
  onQr,
}: {
  server: Homeserver;
  resume: AccountId | null;
  onCreate: () => void;
  onQr: () => void;
}) {
  const offered = otherWays(server, resume);

  if (!offered.create && !offered.qr) return null;

  return (
    <View style={styles.others}>
      {offered.create ? (
        <Button
          testID="create-account"
          title={t('mobile.accounts.create_account')}
          kind="plain"
          onPress={onCreate}
        />
      ) : null}
      {offered.qr ? (
        <Button
          testID="server-sign-in-qr"
          title={t('mobile.accounts.sign_in_qr')}
          icon="qrScan"
          kind="plain"
          onPress={onQr}
        />
      ) : null}
    </View>
  );
}

/**
 * Creating an account on a password server: the form, then each thing the server asks (a
 * registration token, agreeing to its policies, an email address to confirm, a step on its page).
 */
function RegisterStep({
  step,
  username,
  onUsername,
  onSubmit,
  onAnswer,
  onReopen,
  onSignIn,
}: {
  step: Extract<SignInStep, { type: 'register' }>;
  username: string;
  onUsername: (text: string) => void;
  onSubmit: (username: string, password: string, confirmation: string) => void;
  onAnswer: (answer: Answer) => void;
  onReopen: () => void;
  onSignIn: () => void;
}) {
  const palette = usePalette();
  const [password, setPassword] = useState('');
  const [confirmation, setConfirmation] = useState('');
  const server = serverName(step.server);

  if (step.question !== null)
    return (
      <RegistrationQuestion
        question={step.question}
        server={server}
        onAnswer={onAnswer}
        onReopen={onReopen}
      />
    );

  const submitting = step.request !== null;
  const submit = () => onSubmit(username, password, confirmation);

  return (
    <View style={styles.step}>
      <Text style={[font.title, { color: palette.text }]}>
        {t('mobile.accounts.create_title', { server })}
      </Text>
      <TextField
        testID="register-username"
        label={t('sign_in.username')}
        value={username}
        onChangeText={onUsername}
        placeholder="alice"
        autoCapitalize="none"
        autoCorrect={false}
        autoComplete="username-new"
        textContentType="username"
        returnKeyType="next"
        editable={!submitting}
      />
      <TextField
        testID="register-password"
        label={t('sign_in.password')}
        value={password}
        onChangeText={setPassword}
        secureTextEntry
        autoComplete="new-password"
        textContentType="newPassword"
        returnKeyType="next"
        editable={!submitting}
      />
      <TextField
        testID="register-confirm"
        label={t('mobile.accounts.register_confirm')}
        value={confirmation}
        onChangeText={setConfirmation}
        secureTextEntry
        autoComplete="new-password"
        textContentType="newPassword"
        returnKeyType="go"
        editable={!submitting}
        onSubmitEditing={submit}
      />
      <Button
        testID="register"
        title={submitting ? t('mobile.accounts.registering') : t('mobile.accounts.create_account')}
        busy={submitting}
        onPress={submit}
      />
      {step.server.methods.password ? (
        <Button
          testID="register-sign-in"
          title={t('mobile.accounts.sign_in_instead')}
          kind="plain"
          onPress={onSignIn}
        />
      ) : null}
    </View>
  );
}

function RegistrationQuestion({
  question,
  server,
  onAnswer,
  onReopen,
}: {
  question: RegistrationStep;
  server: string;
  onAnswer: (answer: Answer) => void;
  onReopen: () => void;
}) {
  const store = useAppStore();
  const palette = usePalette();
  const [token, setToken] = useState('');
  const [email, setEmail] = useState('');

  switch (question.type) {
    case 'Token':
      return (
        <View style={styles.step}>
          <Text style={[font.body, { color: palette.textMuted }]}>
            {t('mobile.accounts.registration_token_hint', { server })}
          </Text>
          <TextField
            testID="registration-token"
            label={t('mobile.accounts.registration_token')}
            value={token}
            onChangeText={setToken}
            autoCapitalize="none"
            autoCorrect={false}
            autoFocus
            returnKeyType="go"
            onSubmitEditing={() => onAnswer({ type: 'RegistrationToken', data: token.trim() })}
          />
          <Button
            testID="registration-token-continue"
            title={t('common.continue')}
            disabled={!token.trim()}
            onPress={() => onAnswer({ type: 'RegistrationToken', data: token.trim() })}
          />
        </View>
      );
    case 'Terms':
      return (
        <View style={styles.step}>
          <Text style={[font.body, { color: palette.textMuted }]}>
            {t('mobile.accounts.terms_hint', { server })}
          </Text>
          <View style={styles.policies}>
            <ListSection>
              {question.data.map((policy) => (
                <ListRow
                  key={policy.url}
                  testID={`policy-${policy.url}`}
                  icon="external"
                  title={policy.name}
                  chevron={false}
                  onPress={() => openInBrowser(store, policy.url)}
                />
              ))}
            </ListSection>
          </View>
          <Button
            testID="registration-agree"
            title={t('mobile.accounts.agree')}
            onPress={() => onAnswer({ type: 'Agree' })}
          />
        </View>
      );
    case 'Email':
      return (
        <View style={styles.step}>
          <Text style={[font.body, { color: palette.textMuted }]}>
            {t('mobile.accounts.registration_email_hint', { server })}
          </Text>
          <TextField
            testID="registration-email"
            label={t('mobile.accounts.registration_email')}
            value={email}
            onChangeText={setEmail}
            placeholder={t('profile.email_placeholder')}
            keyboardType="email-address"
            autoComplete="email"
            textContentType="emailAddress"
            autoCapitalize="none"
            autoCorrect={false}
            autoFocus
            returnKeyType="go"
            onSubmitEditing={() => onAnswer({ type: 'Email', data: email })}
          />
          <Button
            testID="registration-email-continue"
            title={t('common.continue')}
            disabled={!email.trim()}
            onPress={() => onAnswer({ type: 'Email', data: email })}
          />
        </View>
      );
    case 'EmailSent':
      return (
        <View style={styles.step}>
          <Text testID="registration-email-sent" style={[font.body, { color: palette.text }]}>
            {t('mobile.accounts.registration_email_sent', { address: question.data.address })}
          </Text>
          {question.data.unconfirmed ? (
            <Text
              testID="registration-email-unconfirmed"
              style={[font.body, { color: palette.warning }]}
            >
              {t('mobile.accounts.registration_email_unconfirmed', { server })}
            </Text>
          ) : null}
          <Button
            testID="registration-email-opened"
            title={t('common.continue')}
            onPress={() => onAnswer({ type: 'Continue' })}
          />
        </View>
      );
    case 'Browser':
      return (
        <View style={styles.step}>
          <Text style={[font.body, { color: palette.textMuted }]}>
            {t('mobile.accounts.registration_browser', { server })}
          </Text>
          <Button
            testID="registration-continue"
            title={t('common.continue')}
            onPress={() => onAnswer({ type: 'Continue' })}
          />
          <Button
            testID="registration-reopen"
            title={t('mobile.accounts.open_page_again')}
            kind="secondary"
            onPress={onReopen}
          />
        </View>
      );
  }
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
      <Button
        testID={`resume-${account}`}
        title={t('session_ended.sign_in')}
        kind="plain"
        onPress={onResume}
      />
      <Button
        testID={`forget-${account}`}
        title={t('common.remove')}
        kind="plain"
        onPress={onRemove}
      />
    </View>
  );
}

/** The resolved homeserver, where it lives if that differs, and a way back to step 1. */
function ServerRow({ server, onChange }: { server: Homeserver; onChange: () => void }) {
  const palette = usePalette();
  const name = serverName(server);
  const host = serverHost(server);

  return (
    <View
      style={[styles.serverRow, { borderColor: palette.divider, backgroundColor: palette.surface }]}
    >
      <Icon name="globe" size={20} color={palette.textMuted} />
      <View style={styles.serverNames}>
        <Text
          testID="server-name"
          style={[font.bodyStrong, { color: palette.text }]}
          numberOfLines={1}
        >
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
        {waiting
          ? t('mobile.accounts.browser_waiting', { server: serverName(server) })
          : t('sign_in.browser_failed')}
      </Text>
      {!waiting ? (
        <Button testID="retry-browser" title={t('sign_in.try_again')} onPress={onRetry} />
      ) : opened ? (
        <Button
          testID="reopen-browser"
          title={t('sign_in.reopen')}
          kind="secondary"
          onPress={onReopen}
        />
      ) : (
        <Button title={t('sign_in.browser_title')} busy onPress={onReopen} />
      )}
      {server.methods.password ? (
        <Button
          testID="use-password"
          title={t('sign_in.use_password')}
          kind="plain"
          onPress={onPassword}
        />
      ) : null}
    </View>
  );
}

const styles = StyleSheet.create({
  screen: { flex: 1 },
  topBar: { flexDirection: 'row', justifyContent: 'flex-start', paddingHorizontal: 8 },
  content: {
    flexGrow: 1,
    justifyContent: 'center',
    padding: 24,
    gap: 24,
    maxWidth: 480,
    width: '100%',
    alignSelf: 'center',
  },
  brand: { flexDirection: 'row', alignItems: 'center', gap: 12 },
  step: { gap: 16 },
  others: { gap: 4, marginTop: -8 },
  policies: { marginHorizontal: -16, marginTop: -22 },
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
