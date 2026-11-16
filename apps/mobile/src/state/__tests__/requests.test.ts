import { CommandFailed, Requests } from '../requests';

describe('Requests', () => {
  it('should resolve when its request finishes well', async () => {
    const requests = new Requests();
    const done = requests.track(1);
    requests.hear({ type: 'Finished', data: { request: 1, result: { Ok: null } } });
    await expect(done).resolves.toBeUndefined();
  });

  it('should reject with the engine error when its request fails', async () => {
    const requests = new Requests();
    const done = requests.track(1);
    requests.hear({ type: 'Finished', data: { request: 1, result: { Err: 'wrong password' } } });
    await expect(done).rejects.toEqual(new CommandFailed('wrong password', 1));
  });

  it('should hand the events answering a request to its sender', () => {
    const requests = new Requests();
    const heard: string[] = [];
    void requests.track(1, (event) => heard.push(event.type));
    requests.hear({ type: 'OpenUrl', data: { request: 1, url: 'https://example.org' } });
    expect(heard).toEqual(['OpenUrl']);
  });

  it('should leave other requests events alone', () => {
    const requests = new Requests();
    void requests.track(1);
    expect(requests.hear({ type: 'OpenUrl', data: { request: 2, url: 'https://example.org' } })).toBe(false);
  });
});
