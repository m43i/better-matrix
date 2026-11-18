import { detachedEngine } from '../engine';

describe('Engine', () => {
  it('should give each command its own request id', () => {
    const { engine } = detachedEngine();
    const first = engine.send({ type: 'HangUp' });
    const second = engine.send({ type: 'ListCallDevices' });
    expect(second).not.toBe(first);
  });

  it('should record what a detached engine was sent', () => {
    const { engine, sent } = detachedEngine();
    engine.send({ type: 'Discover', data: { server: 'matrix.org' } });
    expect(sent.map((s) => s.command)).toEqual([
      { type: 'Discover', data: { server: 'matrix.org' } },
    ]);
  });

  it('should hand bridge json to every listener as an event', () => {
    const { engine } = detachedEngine();
    const heard: unknown[] = [];
    engine.subscribe((event) => heard.push(event));
    engine.emitJson('{"type":"AccountRemoved","data":"@a:example.org"}');
    expect(heard).toEqual([{ type: 'AccountRemoved', data: '@a:example.org' }]);
  });

  it('should hand a batch of bridge json to every listener as its events in order', () => {
    const { engine } = detachedEngine();
    const heard: unknown[] = [];
    engine.subscribe((event) => heard.push(event));
    engine.emitJson(
      '[{"type":"AccountRemoved","data":"@a:x"},{"type":"AccountRemoved","data":"@b:x"}]',
    );
    expect(heard).toEqual([
      { type: 'AccountRemoved', data: '@a:x' },
      { type: 'AccountRemoved', data: '@b:x' },
    ]);
  });

  it('should hand a batch listener the events that came together at once', () => {
    const { engine } = detachedEngine();
    const batches: number[] = [];
    engine.subscribeBatches((events) => batches.push(events.length));
    engine.emitJson(
      '[{"type":"AccountRemoved","data":"@a:x"},{"type":"AccountRemoved","data":"@b:x"}]',
    );
    engine.emitJson('{"type":"AccountRemoved","data":"@c:x"}');
    expect(batches).toEqual([2, 1]);
  });

  it('should stop telling a listener once it unsubscribed', () => {
    const { engine } = detachedEngine();
    const heard: unknown[] = [];
    const stop = engine.subscribe((event) => heard.push(event));
    stop();
    engine.emitJson('{"type":"AccountRemoved","data":"@a:example.org"}');
    expect(heard).toEqual([]);
  });
});
