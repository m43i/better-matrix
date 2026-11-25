// Opening one of a room's widgets in the system's browser rather than the app: the engine gives
// the address (a meeting's is the one that joins it), and a failure becomes a notice. A meeting
// server asking to confirm who the user is gets an alert naming it: Allow or Deny answer the
// run; dismissing the alert cancels it.

import type { AccountId, WidgetPermission } from '@better-matrix/matrix-native/types';
import { Alert, Linking } from 'react-native';

import { t } from '../../i18n';
import { openWidgetInBrowser, type PermissionAnswer } from '../../state/actions/widgets';
import { addNotice } from '../../state/reducer';
import { CommandFailed } from '../../state/requests';
import type { Store } from '../../state/store';
import { permissionText } from '../../state/widgets';

/** Asks in an alert whether `widget` may do what `permissions` say. */
function askInAlert(widget: string, permissions: WidgetPermission[]): Promise<PermissionAnswer> {
  return new Promise((resolve) => {
    Alert.alert(
      t('mobile.widgets.permissions_title', { widget }),
      permissions.map(permissionText).join('\n'),
      [
        { text: t('mobile.widgets.deny'), style: 'cancel', onPress: () => resolve(false) },
        { text: t('mobile.widgets.allow'), onPress: () => resolve(true) },
      ],
      { cancelable: true, onDismiss: () => resolve(null) },
    );
  });
}

/** Opens a widget's address in the system's browser; a failure becomes a notice. */
export function openOutside(store: Store, url: string): void {
  Linking.openURL(url).catch((cause: unknown) => {
    const error = cause instanceof Error ? cause.message : String(cause);
    store.update((s) => addNotice(s, t('notice.browser', { error })));
  });
}

export function openWidgetInSystemBrowser(
  store: Store,
  account: AccountId,
  roomId: string,
  widgetId: string,
): void {
  const failed = (cause: unknown) => {
    // Cancelled: the user dismissed the question.
    if (cause instanceof CommandFailed && cause.cancelled) return;
    const error = cause instanceof Error ? cause.message : String(cause);
    store.update((s) => addNotice(s, t('notice.browser', { error })));
  };

  openWidgetInBrowser(
    store,
    account,
    roomId,
    widgetId,
    (url) => {
      Linking.openURL(url).catch(failed);
    },
    askInAlert,
  ).catch(failed);
}
