// The verification sheet: verifying this session with another of the user's sessions, or another
// person, by scanning a QR code one way or the other or by comparing emoji. Opened without a flow
// it asks the other sessions (or with `userId`, that person); opened for a flow it follows a
// request that came in (accept or decline). Closing it stops a verification under way.

import type { AccountId, SasEmoji, VerificationStep } from '@better-matrix/matrix-native/types';
import { router, useLocalSearchParams } from 'expo-router';
import { useEffect, useRef, useState, useSyncExternalStore } from 'react';
import { ScrollView, StyleSheet, Text, useWindowDimensions, View } from 'react-native';

import { t } from '../i18n';
import { routes } from '../routes';
import {
  acceptVerification,
  AskedVerification,
  cancelVerification,
  compareEmoji,
  confirmVerification,
  followedVerification,
  scanVerificationCode,
  type VerificationAction,
  verificationOver,
  verificationView,
} from '../state/actions/security';
import { useAppStore, useStore } from '../state/react';
import { knownPerson } from '../state/selectors';
import { font } from '../theme/palette';
import { Button, type ButtonKind } from '../ui/components';
import { Icon } from '../ui/icons';
import { QrCode } from '../ui/qr/QrCode';
import { QrScanner } from '../ui/qr/QrScanner';
import { usePalette } from '../ui/theme';

export default function VerifyScreen() {
  const store = useAppStore();
  const palette = usePalette();
  const { width } = useWindowDimensions();
  const params = useLocalSearchParams<{ account: AccountId; flowId?: string; userId?: string }>();
  const account = params.account;
  const flowId = params.flowId ?? null;
  const userId = params.userId ?? null;

  // Opened without a flow, the sheet asks for one and follows exactly the flow that started.
  const [asked] = useState(() =>
    flowId === null ? new AskedVerification(store, account, userId) : null,
  );

  const started = useSyncExternalStore(
    asked?.subscribe ?? noSubscription,
    asked?.flowId ?? noFlow,
    asked?.flowId ?? noFlow,
  );

  const verification = useStore((s) =>
    followedVerification(s.verifications, account, flowId ?? started),
  );

  // Who is on the other side, when it's another person: from their request, or the profile
  // that opened the sheet (a member of a room the account shares with them).
  const profile = useStore((s) => (userId === null ? undefined : knownPerson(s, account, userId)));

  const person = verification?.user ?? profile ?? null;
  const name = person?.name ?? userId ?? '';

  const others = person !== null || userId !== null;
  const step = verification?.step ?? null;
  const view = verificationView(step, others);
  const [scanning, setScanning] = useState(false);

  // Opened to verify this session or someone: ask, once. Closing the sheet stops what is under
  // way: the request or the verification it started, or the one it was opened for.
  const latest = useRef(verification);
  useEffect(() => {
    latest.current = verification;
  }, [verification]);
  useEffect(() => {
    asked?.start();

    return () => {
      if (asked) {
        asked.close();

        return;
      }

      const current = latest.current;

      if (current && !verificationOver(current.step))
        cancelVerification(store, current.account, current.flowId);
    };
  }, [store, asked]);

  // A step that isn't the code anymore closes the camera.
  if (scanning && step?.type !== 'Ready') setScanning(false);

  const act = (action: VerificationAction) => {
    const flow = latest.current;

    switch (action) {
      case 'accept':
        if (flow) acceptVerification(store, flow.account, flow.flowId);

        return;
      case 'scan':
        setScanning(true);

        return;
      case 'emoji':
        setScanning(false);

        if (flow) compareEmoji(store, flow.account, flow.flowId);

        return;
      case 'match':
      case 'mismatch':
      case 'scanned':
      case 'notScanned':
        if (flow)
          confirmVerification(
            store,
            flow.account,
            flow.flowId,
            action === 'match' || action === 'scanned',
          );

        return;
      case 'decline':
      case 'cancel':
      case 'done':
      case 'close':
        // Leaving stops it (see above).
        router.back();

        return;
    }
  };

  const scanned = (code: number[]) => {
    const flow = latest.current;
    setScanning(false);

    if (flow) scanVerificationCode(store, flow.account, flow.flowId, code);
  };

  return (
    <ScrollView
      style={{ backgroundColor: palette.bg }}
      contentContainerStyle={styles.sheet}
      testID="verify"
    >
      <View style={styles.heading}>
        <Icon
          name={
            step?.type === 'Done' ? 'verified' : step?.type === 'Cancelled' ? 'alert' : 'shield'
          }
          size={40}
          color={
            step?.type === 'Done'
              ? palette.success
              : step?.type === 'Cancelled'
                ? palette.warning
                : palette.accent
          }
        />
        <Text style={[font.title, { color: palette.text, textAlign: 'center' }]}>
          {t(view.title, { name })}
        </Text>
        <Text
          testID="verify-message"
          style={[font.body, { color: palette.textMuted, textAlign: 'center' }]}
        >
          {message(step, others ? name : null)}
        </Text>
      </View>
      {step?.type === 'Compare' ? <EmojiGrid emojis={step.data} /> : null}
      {step?.type === 'Ready' ? (
        scanning ? (
          <QrScanner testID="verify-scanner" onCode={scanned} />
        ) : step.data.qr ? (
          <QrCode testID="verify-qr" qr={step.data.qr} size={Math.min(width - 96, 300)} />
        ) : null
      ) : null}
      <View style={styles.buttons}>
        {view.actions
          .filter((action) => !(scanning && action === 'scan'))
          .map((action) => (
            <Button
              key={action}
              testID={`verify-${action}`}
              title={label(action)}
              kind={kind(action)}
              onPress={() => act(action)}
            />
          ))}
      </View>
      {!others && (step === null || step.type === 'Waiting') ? (
        <Button
          testID="verify-use-key"
          title={t('account.verify_key')}
          kind="plain"
          onPress={() => router.replace(routes.recovery(account))}
        />
      ) : null}
    </ScrollView>
  );
}

/** A sheet opened for a flow asks for none. */
const noSubscription = () => () => undefined;

const noFlow = () => null;

/** What the sheet says at a step; `name` is the other person's, when it isn't the user's session. */
function message(step: VerificationStep | null, name: string | null): string {
  switch (step?.type) {
    case 'Incoming':
      return name === null
        ? t('verify.incoming', { device: step.data.other_device })
        : t('mobile.accounts.verify_person_incoming', { name, device: step.data.other_device });
    case 'Ready': {
      const { qr, scan } = step.data;

      if (name === null) {
        if (qr === null) return t('mobile.accounts.verify_scan_only');

        return scan
          ? t('mobile.accounts.verify_show_or_scan')
          : t('mobile.accounts.verify_show_code');
      }

      if (qr === null) return t('mobile.accounts.verify_person_scan_only', { name });

      return scan
        ? t('mobile.accounts.verify_person_show_or_scan', { name })
        : t('mobile.accounts.verify_person_show_code', { name });
    }

    case 'Scanned':
      return name === null
        ? t('mobile.accounts.verify_scanned')
        : t('mobile.accounts.verify_person_scanned', { name });
    case 'Compare':
      return name === null
        ? t('verify.compare')
        : t('mobile.accounts.verify_person_compare', { name });
    case 'Confirmed':
      return name === null
        ? t('verify.confirmed')
        : t('mobile.accounts.verify_person_confirmed', { name });
    case 'Done':
      return name === null ? t('verify.done') : t('mobile.accounts.verify_person_done', { name });
    case 'Cancelled':
      return t('verify.cancelled', { reason: step.data });
    case 'Waiting':
    case undefined:
      return name === null
        ? t('verify.waiting')
        : t('mobile.accounts.verify_person_waiting', { name });
  }
}

function label(action: VerificationAction): string {
  switch (action) {
    case 'accept':
      return t('verify.accept');
    case 'decline':
      return t('verify.decline');
    case 'cancel':
      return t('common.cancel');
    case 'scan':
      return t('mobile.accounts.verify_scan');
    case 'emoji':
      return t('mobile.accounts.verify_emoji');
    case 'scanned':
      return t('mobile.accounts.verify_scanned_yes');
    case 'notScanned':
      return t('mobile.accounts.verify_scanned_no');
    case 'match':
      return t('verify.match');
    case 'mismatch':
      return t('verify.mismatch');
    case 'done':
      return t('common.done');
    case 'close':
      return t('common.close');
  }
}

function kind(action: VerificationAction): ButtonKind {
  switch (action) {
    case 'accept':
    case 'match':
    case 'scanned':
    case 'scan':
    case 'done':
      return 'primary';
    case 'emoji':
    case 'decline':
    case 'cancel':
    case 'mismatch':
    case 'notScanned':
    case 'close':
      return 'secondary';
  }
}

/** The seven emoji to compare, each with its name, in a grid. */
function EmojiGrid({ emojis }: { emojis: SasEmoji[] }) {
  const palette = usePalette();

  return (
    <View style={styles.grid} testID="verify-emoji">
      {emojis.map((emoji, index) => (
        <View key={`${index}-${emoji.symbol}`} style={styles.emoji}>
          <Text style={styles.symbol} allowFontScaling={false}>
            {emoji.symbol}
          </Text>
          <Text
            style={[font.meta, { color: palette.textMuted, textAlign: 'center' }]}
            numberOfLines={2}
          >
            {emoji.description}
          </Text>
        </View>
      ))}
    </View>
  );
}

const styles = StyleSheet.create({
  sheet: { padding: 24, paddingTop: 32, gap: 24 },
  heading: { alignItems: 'center', gap: 10 },
  grid: { flexDirection: 'row', flexWrap: 'wrap', justifyContent: 'center', rowGap: 16 },
  emoji: { width: '25%', alignItems: 'center', gap: 4 },
  symbol: { fontSize: 38 },
  buttons: { gap: 10 },
});
