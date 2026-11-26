// What the administration screens call things: join rules, history choices, permissions and
// levels, in the catalog's words.

import type { HistoryVisibility, JoinRule, Permission } from '@better-matrix/matrix-native/types';

import { type Key, t } from '../../i18n';
import { levelChoice } from '../../state/admin';

const JOIN_RULES: Record<JoinRule, [Key, Key]> = {
  Invite: ['mobile.admin.join.Invite', 'mobile.admin.join.Invite_hint'],
  Restricted: ['mobile.admin.join.Restricted', 'mobile.admin.join.Restricted_hint'],
  Knock: ['mobile.admin.join.Knock', 'mobile.admin.join.Knock_hint'],
  Public: ['mobile.admin.join.Public', 'mobile.admin.join.Public_hint'],
};

/** A join rule's name: Private, Space members, Ask to join, Public. */
export function joinRuleLabel(rule: JoinRule): string {
  return t(JOIN_RULES[rule][0]);
}

/** Who a join rule lets in, as a sentence. */
export function joinRuleHint(rule: JoinRule): string {
  return t(JOIN_RULES[rule][1]);
}

const HISTORY: Record<HistoryVisibility, Key> = {
  WorldReadable: 'mobile.admin.history.WorldReadable',
  Shared: 'mobile.admin.history.Shared',
  Invited: 'mobile.admin.history.Invited',
  Joined: 'mobile.admin.history.Joined',
};

/** The history choices, from the most open. */
export const HISTORY_CHOICES: readonly HistoryVisibility[] = [
  'WorldReadable',
  'Shared',
  'Invited',
  'Joined',
];

/** A history choice in a few words. */
export function historyChoiceLabel(history: HistoryVisibility): string {
  return t(HISTORY[history]);
}

const PERMISSIONS: Record<Permission, Key> = {
  DefaultLevel: 'mobile.admin.permission.DefaultLevel',
  SendMessages: 'mobile.admin.permission.SendMessages',
  Invite: 'mobile.admin.permission.Invite',
  Kick: 'mobile.admin.permission.Kick',
  Ban: 'mobile.admin.permission.Ban',
  RemoveMessages: 'mobile.admin.permission.RemoveMessages',
  MentionRoom: 'mobile.admin.permission.MentionRoom',
  ChangeSettings: 'mobile.admin.permission.ChangeSettings',
  Name: 'mobile.admin.permission.Name',
  Topic: 'mobile.admin.permission.Topic',
  Avatar: 'mobile.admin.permission.Avatar',
  Addresses: 'mobile.admin.permission.Addresses',
  Access: 'mobile.admin.permission.Access',
  LiveLocation: 'permission.live_location',
  Pins: 'mobile.admin.permission.Pins',
  Permissions: 'mobile.admin.permission.Permissions',
  Encryption: 'mobile.admin.permission.Encryption',
  Upgrade: 'mobile.admin.permission.Upgrade',
  SpaceRooms: 'mobile.admin.permission.SpaceRooms',
};

/** What a permission allows. */
export function permissionLabel(permission: Permission): string {
  return t(PERMISSIONS[permission]);
}

/** A power level as a role's name, or its number when it is no role's. */
export function levelLabel(level: number): string {
  switch (levelChoice(level)) {
    case 'Admin':
      return t('mobile.admin.level.Admin');
    case 'Moderator':
      return t('mobile.admin.level.Moderator');
    case 'Member':
      return t('mobile.admin.level.Member');
    case 'Custom':
      return t('mobile.admin.level.custom', { level });
  }
}
