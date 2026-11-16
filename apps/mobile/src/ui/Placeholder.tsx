// A screen not built yet: says which, so navigation can be tried before every screen exists.
import { EmptyState } from './components';
import { usePalette } from './theme';
import { View } from 'react-native';

export function Placeholder({ name }: { name: string }) {
  const palette = usePalette();
  return (
    <View style={{ flex: 1, backgroundColor: palette.bg }}>
      <EmptyState icon="info" title={name} />
    </View>
  );
}
