// The verification sheet: verifying this session with another of the user's sessions by
// comparing emoji. Opened without a flow it asks the other sessions; opened for a flow it follows
// a request that came in (accept or decline). Closing it stops a verification under way.

import type { AccountId, SasEmoji } from '@better-matrix/matrix-native/types';
import { router, useLocalSearchParams } from 'expo-router';
import { useEffect, useRef, useState } from 'react';
import { ScrollView, StyleSheet, Text, View } from 'react-native';

import { t } from '../i18n';
import { routes } from '../routes';
import {
  acceptVerification,
  cancelVerification,
  confirmVerification,
  followedVerification,
  type VerificationAction,
  verificationOver,
  verificationView,
  verifyThisSession,
} from '../state/actions/security';
import { useAppStore, useStore } from '../state/react';
import { font } from '../theme/palette';
import { Button, type ButtonKind } from '../ui/components';
import { Icon } from '../ui/icons';
import { usePalette } from '../ui/theme';

export default function VerifyScreen() {
  const store = useAppStore();
  const palette = usePalette();
  const params = useLocalSearchParams<{ account: AccountId; flowId?: string }>();
  const account = params.account;
  const flowId = params.flowId ?? null;
  // The account's verifications before this sheet opened: the one it starts is the next.
  const [known] = useState(() =>
    store
      .getState()
      .verifications.filter((v) => v.account === account)
      .map((v) => v.flowId),
  );
  const verification = useStore((s) => followedVerification(s.verifications, account, flowId, known));
  const step = verification?.step ?? null;
  const view = verificationView(step);

  // Opened to verify this session: ask the other sessions, once.
  const request = useRef<number | null>(null);
  useEffect(() => {
    if (flowId === null && request.current === null) request.current = verifyThisSession(store, account);
  }, [store, account, flowId]);

  // Closing the sheet stops what is under way: a request still unanswered, or the verification.
  const latest = useRef(verification);
  useEffect(() => {
    latest.current = verification;
  }, [verification]);
  useEffect(
    () => () => {
      const current = latest.current;
      if (current) {
        if (!verificationOver(current.step)) cancelVerification(store, current.account, current.flowId);
      } else if (request.current !== null) {
        store.send({ type: 'Cancel', data: request.current }, { quiet: true });
      }
    },
    [store],
  );

  const act = (action: VerificationAction) => {
    const flow = latest.current;
    switch (action) {
      case 'accept':
        if (flow) acceptVerification(store, flow.account, flow.flowId);
        return;
      case 'match':
      case 'mismatch':
        if (flow) confirmVerification(store, flow.account, flow.flowId, action === 'match');
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

  const message = (() => {
    switch (step?.type) {
      case 'Incoming':
        return t('verify.incoming', { device: step.data.other_device });
      case 'Compare':
        return t('verify.compare');
      case 'Confirmed':
        return t('verify.confirmed');
      case 'Done':
        return t('verify.done');
      case 'Cancelled':
        return t('verify.cancelled', { reason: step.data });
      case 'Waiting':
      case undefined:
        return t('verify.waiting');
    }
  })();

  return (
    <ScrollView style={{ backgroundColor: palette.bg }} contentContainerStyle={styles.sheet} testID="verify">
      <View style={styles.heading}>
        <Icon
          name={step?.type === 'Done' ? 'verified' : step?.type === 'Cancelled' ? 'alert' : 'shield'}
          size={40}
          color={step?.type === 'Done' ? palette.success : step?.type === 'Cancelled' ? palette.warning : palette.accent}
        />
        <Text style={[font.title, { color: palette.text, textAlign: 'center' }]}>{t(view.title)}</Text>
        <Text testID="verify-message" style={[font.body, { color: palette.textMuted, textAlign: 'center' }]}>
          {message}
        </Text>
      </View>
      {step?.type === 'Compare' ? <EmojiGrid emojis={step.data} /> : null}
      <View style={styles.buttons}>
        {view.actions.map((action) => (
          <Button key={action} testID={`verify-${action}`} title={label(action)} kind={kind(action)} onPress={() => act(action)} />
        ))}
      </View>
      {step === null || step.type === 'Waiting' ? (
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

function label(action: VerificationAction): string {
  switch (action) {
    case 'accept':
      return t('verify.accept');
    case 'decline':
      return t('verify.decline');
    case 'cancel':
      return t('common.cancel');
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
  return action === 'accept' || action === 'match' || action === 'done' ? 'primary' : 'secondary';
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
          <Text style={[font.meta, { color: palette.textMuted, textAlign: 'center' }]} numberOfLines={2}>
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
