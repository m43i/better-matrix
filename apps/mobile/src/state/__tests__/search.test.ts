import vectors from '../../../../../packages/matrix-core/src/search/matches.json';
import { fuzzyMatch } from '../search';

it.each(vectors)(
  'should highlight $query in $text using the shared Unicode positions',
  ({ text, query, positions }) => {
    expect(fuzzyMatch(text, query)?.positions ?? null).toEqual(positions);
  },
);

it('should rank contiguous word matches before scattered characters', () => {
  expect(fuzzyMatch('green', 'gr')!.rank).toBeLessThan(fuzzyMatch('great room', 'grm')!.rank);
});
