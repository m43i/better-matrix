// Safety (ADR 0011): ignoring people, choosing where media shows and which invitations list,
// reporting to the homeserver's admins, and showing one message's hidden media.

import type {
  AccountId,
  Invitation,
  ReportTarget,
  SafetyChange,
} from '@better-matrix/matrix-native/types';

import { t } from '../../i18n';
import { addNotice, addPendingSafety, settlePendingSafety } from '../reducer';
import { CommandFailed } from '../requests';
import { revealKey } from '../safety';
import type { Store } from '../store';
import { answerInvitation, type InvitationAnswer } from './invitations';

/** Tells the changes on their way apart, the same change made twice included. */
let nextPending = 1;

/**
 * Makes one change on every session. It shows at once, on top of the engine's last report; the
 * engine's `Safety` answer confirms it. A failure says why and leaves the reported choices with
 * the other changes still on their way (`settlePendingSafety`).
 */
export async function changeSafety(
  store: Store,
  account: AccountId,
  change: SafetyChange,
): Promise<void> {
  if (!store.getState().accounts.some((a) => a.summary.id === account)) return;
  const pending = { id: nextPending++, change };
  store.update((s) => addPendingSafety(s, account, pending));

  try {
    await store.run({ type: 'SetSafety', data: { account, change } });
  } catch (error) {
    store.update((s) => {
      const back = settlePendingSafety(s, account, pending.id);

      // A cancelled request says nothing: the user did it.
      if (!(error instanceof CommandFailed) || error.cancelled) return back;

      return addNotice(back, t('mobile.safety.save_failed', { error: error.message }), account);
    });
    throw error;
  }

  store.update((s) => settlePendingSafety(s, account, pending.id));
}

/** Ignores someone, or stops ignoring them. */
export function setIgnored(
  store: Store,
  account: AccountId,
  userId: string,
  ignored: boolean,
): Promise<void> {
  return changeSafety(store, account, { type: 'Ignore', data: { user_id: userId, ignored } });
}

/** Whom a report is about, when it is about someone the user may also ignore. */
export function reportedPerson(target: ReportTarget, sender: string | null): string | null {
  switch (target.type) {
    case 'Person':
      return target.data.user_id;
    case 'Message':
      return sender;
    case 'Room':
      return null;
  }
}

/**
 * Reports to the homeserver's admins; `ignore` also ignores that person once the report went.
 * Success says so; a failure says why and rejects, so the sheet stays to try again.
 */
export async function report(
  store: Store,
  account: AccountId,
  target: ReportTarget,
  reason: string,
  ignore: string | null = null,
): Promise<void> {
  try {
    await store.run({ type: 'Report', data: { account, target, reason: reason.trim() } });
  } catch (error) {
    if (error instanceof CommandFailed && !error.cancelled) {
      const failed = error;
      store.update((s) => addNotice(s, t('notice.report', { error: failed.message }), account));
    }

    throw error;
  }

  store.update((s) => addNotice(s, t('mobile.safety.reported'), account));

  if (ignore !== null) await setIgnored(store, account, ignore, true).catch(() => {});
}

/**
 * What the report sheet does once its report ended: a report sent closes it, unless the user
 * already left it (`shown` false), when nothing moves; a failure stays to try again.
 */
export function afterReport(sent: boolean, shown: boolean): 'close' | 'stay' {
  return sent && shown ? 'close' : 'stay';
}

/** Declines an invitation and ignores whoever sent it (Ignore on the invitation sheet). */
export async function ignoreInviter(
  store: Store,
  account: AccountId,
  invitation: Invitation,
): Promise<InvitationAnswer> {
  if (invitation.inviter) {
    try {
      await setIgnored(store, account, invitation.inviter.id, true);
    } catch {
      // The notice said why; the invitation stays to try again.
      return { kind: 'failed', error: null };
    }
  }

  return answerInvitation(store, account, invitation.room_id, false);
}

/** Shows one message's hidden media until the app restarts. */
export function revealMedia(store: Store, account: AccountId, eventId: string): void {
  const key = revealKey(account, eventId);
  store.update((s) =>
    s.revealedMedia[key] ? s : { ...s, revealedMedia: { ...s.revealedMedia, [key]: true } },
  );
}
