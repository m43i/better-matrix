import { Launches } from '../launches';

function launches() {
  let next = 0;

  return new Launches(() => `nonce-${++next}`);
}

describe('Launches', () => {
  it('should allow a launch the app registered', () => {
    const registry = launches();
    expect(registry.consume(registry.register())).toBe(true);
  });

  it('should allow each launch once', () => {
    const registry = launches();
    const nonce = registry.register();
    registry.consume(nonce);
    expect(registry.consume(nonce)).toBe(false);
  });

  it('should refuse a launch nobody registered', () => {
    expect(launches().consume('nonce-1')).toBe(false);
  });

  it('should refuse a screen opened without a launch', () => {
    expect(launches().consume(undefined)).toBe(false);
  });
});
