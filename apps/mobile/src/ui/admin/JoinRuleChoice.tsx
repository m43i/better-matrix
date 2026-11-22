// Who may join a room or space, as a single choice: each rule with a line on whom it lets in.
// The new-room form and the settings' Access part share it.

import type { JoinRule } from '@better-matrix/matrix-native/types';
import { View } from 'react-native';

import { t } from '../../i18n';
import { Divider, ListRow, ListSection } from '../components';
import { joinRuleHint, joinRuleLabel } from './labels';

export function JoinRuleChoice({
  rules,
  value,
  onChange,
  footer,
  testID,
}: {
  rules: readonly JoinRule[];
  value: JoinRule | null;
  onChange: (rule: JoinRule) => void;
  footer?: string;
  testID: string;
}) {
  return (
    <ListSection title={t('mobile.admin.join.title')} footer={footer}>
      {rules.map((rule, index) => (
        <View key={rule}>
          {index > 0 ? <Divider inset={16} /> : null}
          <ListRow
            testID={`${testID}-${rule}`}
            title={joinRuleLabel(rule)}
            subtitle={joinRuleHint(rule)}
            selected={value === rule}
            chevron={false}
            onPress={value === rule ? undefined : () => onChange(rule)}
          />
        </View>
      ))}
    </ListSection>
  );
}
