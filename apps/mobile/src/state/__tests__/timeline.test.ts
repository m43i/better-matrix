import { applyDiffs } from '../timeline';
import { entry } from '../../test/fixtures';

const ids = (entries: ReturnType<typeof applyDiffs>) => entries.map((e) => e.id);

describe('applyDiffs', () => {
  it('should insert at an index', () => {
    expect(ids(applyDiffs([entry('a'), entry('c')], [{ type: 'Insert', data: { index: 1, entry: entry('b') } }]))).toEqual(['a', 'b', 'c']);
  });

  it('should replace at an index', () => {
    expect(ids(applyDiffs([entry('a')], [{ type: 'Set', data: { index: 0, entry: entry('z') } }]))).toEqual(['z']);
  });

  it('should remove at an index', () => {
    expect(ids(applyDiffs([entry('a'), entry('b')], [{ type: 'Remove', data: { index: 0 } }]))).toEqual(['b']);
  });

  it('should keep only the first entries on truncate', () => {
    expect(ids(applyDiffs([entry('a'), entry('b'), entry('c')], [{ type: 'Truncate', data: { length: 1 } }]))).toEqual(['a']);
  });

  it('should add at both ends', () => {
    const diffs = [{ type: 'PushFront' as const, data: entry('a') }, { type: 'PushBack' as const, data: entry('c') }];
    expect(ids(applyDiffs([entry('b')], diffs))).toEqual(['a', 'b', 'c']);
  });

  it('should leave the entries it was given untouched', () => {
    const before = [entry('a')];
    applyDiffs(before, [{ type: 'Clear' }]);
    expect(ids(before)).toEqual(['a']);
  });
});
