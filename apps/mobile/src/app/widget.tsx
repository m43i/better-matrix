// A widget running in the app (full screen, from a room's widgets, the room header's video button,
// the room menu or `/meet`): its page in a web view, under a header with its name, Close and, for
// one of the room's widgets, Open in browser. The integration manager's terms come first when
// they weren't agreed to; a widget asking for permissions gets a sheet over its page; on Android a
// meeting that moves to another host asks once for the camera and microphone there (iOS's web
// view asks for each site itself). Closing the screen stops the run; a run that ends by itself,
// or whose page fails to load, says so or closes. Only the app opens this screen: every run needs
// its one-time launch (`openWidget`), so a link opens nothing.

import type { AccountId, Policy, WidgetAccess } from '@better-matrix/matrix-native/types';
import { Stack, useLocalSearchParams, useRouter } from 'expo-router';
import { useEffect, useState, useSyncExternalStore } from 'react';
import { Alert, Linking, Platform, ScrollView, StyleSheet, Text, View } from 'react-native';

import { t } from '../i18n';
import { widgetTarget } from '../routes';
import { WidgetRun } from '../state/actions/widgets';
import { useAppStore, useStore } from '../state/react';
import { account as accountOf } from '../state/selectors';
import { accessContext, accessText, type WidgetTarget } from '../state/widgets';
import { font } from '../theme/palette';
import { openInBrowser } from '../ui/accounts/browser';
import { Button, EmptyState, IconButton, ListRow, ListSection } from '../ui/components';
import { HeaderClose } from '../ui/HeaderButtons';
import { usePalette } from '../ui/theme';
import { openWidgetInSystemBrowser } from '../ui/widgets/browser';
import { bridgeToken, widgetLaunches } from '../ui/widgets/launch';
import { PermissionSheet } from '../ui/widgets/PermissionSheet';
import { WidgetPageView } from '../ui/widgets/WidgetPageView';

export default function WidgetScreen() {
  const params = useLocalSearchParams<{
    account: AccountId;
    roomId: string;
    widgetId?: string;
    permissions?: string;
    meeting?: string;
    integrations?: string;
    launch?: string;
  }>();

  const target = widgetTarget(params);

  // Used up as the screen opens: the same launch can't start a second run, and a link has none.
  const [launched] = useState(() => widgetLaunches.consume(params.launch));

  if (!target) return <EmptyState icon="widgets" title={t('widgets.none')} />;

  if (!launched) return <EmptyState icon="widgets" title={t('mobile.widgets.open_from_room')} />;

  return <Running account={params.account} roomId={params.roomId} target={target} />;
}

function Running({
  account,
  roomId,
  target,
}: {
  account: AccountId;
  roomId: string;
  target: WidgetTarget;
}) {
  const store = useAppStore();
  const router = useRouter();
  const palette = usePalette();

  // Android's web view grants the camera by host, so a meeting on a new host asks; iOS asks itself.
  const [run] = useState(
    () => new WidgetRun(store, account, roomId, target, bridgeToken(), Platform.OS === 'android'),
  );

  const { view, asking, captureHost, askingCapture } = useSyncExternalStore(
    run.subscribe,
    run.snapshot,
    run.snapshot,
  );

  const widgetId = target.type === 'widget' ? target.widgetId : null;

  const name = useStore((s) =>
    widgetId === null
      ? undefined
      : accountOf(s, account)?.widgets[roomId]?.widgets.find((w) => w.id === widgetId)?.name,
  );

  useEffect(() => {
    run.start();

    return () => run.cancel();
  }, [run]);

  // A meeting on a host it wasn't opened on asks once for the camera and microphone there.
  useEffect(() => {
    if (askingCapture === null) return;

    Alert.alert(
      t('mobile.widgets.capture_title', { host: askingCapture }),
      undefined,
      [
        {
          text: t('mobile.widgets.deny'),
          style: 'cancel',
          onPress: () => run.answerCapture(askingCapture, false),
        },
        {
          text: t('mobile.widgets.allow'),
          onPress: () => run.answerCapture(askingCapture, true),
        },
      ],
      { cancelable: true, onDismiss: () => run.answerCapture(askingCapture, false) },
    );
  }, [askingCapture, run]);

  // A closed widget leaves the screen. A browser handoff leaves only once the OS accepts it.
  const browserUrl = view.type === 'browser' ? view.url : null;

  useEffect(() => {
    if (view.type === 'ended' && router.canGoBack()) router.back();
  }, [view.type, router]);

  useEffect(() => {
    if (browserUrl === null) return;
    let onScreen = true;

    void Linking.openURL(browserUrl).then(
      () => {
        if (onScreen && router.canGoBack()) router.back();
      },
      () => {
        if (onScreen) run.browserFailed(t('widgets.browser_failed'));
      },
    );

    return () => {
      onScreen = false;
    };
  }, [browserUrl, router, run]);

  const title =
    view.type === 'page'
      ? view.page.title
      : target.type === 'integrations'
        ? t('mobile.widgets.integrations_title')
        : target.type === 'meeting'
          ? t('mobile.widgets.start_meeting')
          : (name ?? '');

  // In the browser instead: this run stops, so the widget doesn't run twice.
  const inBrowser = () => {
    if (widgetId === null) return;
    run.cancel();
    router.back();
    openWidgetInSystemBrowser(store, account, roomId, widgetId);
  };

  return (
    <>
      <Stack.Screen
        options={{
          title,
          headerLeft: () => <HeaderClose onPress={() => router.back()} testID="widget-close" />,
          headerRight:
            widgetId === null
              ? undefined
              : () => (
                  <IconButton
                    testID="widget-browser"
                    icon="external"
                    label={t('mobile.widgets.open_browser')}
                    color={palette.textMuted}
                    onPress={inBrowser}
                  />
                ),
        }}
      />
      <View style={[styles.screen, { backgroundColor: palette.bg }]} testID="widget">
        {view.type === 'page' ? (
          <WidgetPageView key={view.load} run={run} page={view.page} captureHost={captureHost} />
        ) : view.type === 'access' ? (
          <AccessPrompt
            access={view.access}
            onConfirm={() => run.confirmAccess()}
            onCancel={() => router.back()}
          />
        ) : view.type === 'terms' ? (
          <Terms policies={view.policies} onAgree={() => run.agree()} />
        ) : view.type === 'failed' ? (
          <View style={styles.failed}>
            <EmptyState
              icon="alert"
              title={title}
              body={t('mobile.widgets.failed', { error: view.error })}
            />
            {/* Starting a meeting again would pin another one. */}
            {target.type === 'meeting' ? null : (
              <Button
                testID="widget-retry"
                title={t('common.try_again')}
                kind="secondary"
                onPress={() => run.start()}
              />
            )}
          </View>
        ) : (
          <EmptyState loading title={t('info.loading')} />
        )}
      </View>
      {asking ? (
        <PermissionSheet
          key={`${asking.request}:${asking.question}`}
          ask={asking}
          onAnswer={(granted) => run.grant(asking, granted)}
        />
      ) : null}
    </>
  );
}

function AccessPrompt({
  access,
  onConfirm,
  onCancel,
}: {
  access: WidgetAccess;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  const palette = usePalette();
  const { title, body, action } = accessText(access);

  return (
    <ScrollView contentContainerStyle={styles.terms} testID="widget-access">
      <Text style={[font.header, { color: palette.text }]}>{title}</Text>
      <Text style={[font.body, { color: palette.textMuted }]}>{accessContext(access)}</Text>
      <Text style={[font.body, { color: palette.text }]}>{body}</Text>
      <Button testID="widget-access-confirm" title={action} onPress={onConfirm} />
      <Button
        testID="widget-access-cancel"
        title={t('common.cancel')}
        kind="secondary"
        onPress={onCancel}
      />
    </ScrollView>
  );
}

/** The integration manager's policies, each opening in the browser, and Agree. */
function Terms({ policies, onAgree }: { policies: Policy[]; onAgree: () => void }) {
  const store = useAppStore();
  const palette = usePalette();

  return (
    <ScrollView contentContainerStyle={styles.terms} testID="widget-terms">
      <Text style={[font.header, styles.padded, { color: palette.text }]}>
        {t('mobile.widgets.terms_title')}
      </Text>
      <Text style={[font.body, styles.padded, { color: palette.textMuted }]}>
        {t('mobile.widgets.terms_hint')}
      </Text>
      <ListSection>
        {policies.map((policy) => (
          <ListRow
            key={policy.url}
            testID={`widget-policy-${policy.url}`}
            icon="external"
            title={policy.name}
            chevron={false}
            onPress={() => openInBrowser(store, policy.url)}
          />
        ))}
      </ListSection>
      <View style={styles.padded}>
        <Button testID="widget-agree" title={t('mobile.widgets.agree')} onPress={onAgree} />
      </View>
    </ScrollView>
  );
}

const styles = StyleSheet.create({
  screen: { flex: 1 },
  failed: { flex: 1, paddingHorizontal: 24, paddingBottom: 32 },
  terms: { paddingVertical: 24, gap: 12 },
  padded: { paddingHorizontal: 16 },
});
