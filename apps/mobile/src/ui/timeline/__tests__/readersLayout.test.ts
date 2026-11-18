import { keepsReadersColumn } from '../readersLayout';

it('should keep the readers column on a typical phone', () => {
  expect(keepsReadersColumn(360)).toBe(true);
});

it('should drop the readers column on a screen too narrow to spare it', () => {
  expect(keepsReadersColumn(320)).toBe(false);
});
