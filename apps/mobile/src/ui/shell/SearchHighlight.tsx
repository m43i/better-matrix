// Nested text keeps wrapping, truncation and accessibility with exact character highlights.

import { Text } from 'react-native';

import { fuzzyMatch } from '../../state/search';
import { usePalette } from '../theme';

export function SearchHighlight({
  text,
  query,
  positions,
}: {
  text: string;
  query?: string;
  positions?: readonly number[];
}) {
  const palette = usePalette();
  const matched = new Set(positions ?? fuzzyMatch(text, query ?? '')?.positions ?? []);
  const runs: { text: string; matched: boolean; start: number }[] = [];
  Array.from(text).forEach((char, i) => {
    const mark = matched.has(i);
    const last = runs[runs.length - 1];

    if (last?.matched === mark) last.text += char;
    else runs.push({ text: char, matched: mark, start: i });
  });

  return (
    <>
      {runs.map((run) => (
        <Text
          key={run.start}
          style={run.matched ? { color: palette.accent, fontWeight: '700' } : undefined}
        >
          {run.text}
        </Text>
      ))}
    </>
  );
}
