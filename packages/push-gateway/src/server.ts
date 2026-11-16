// The gateway as a plain Bun server: `PORT` (default 5000), `APP_IDS`, `EXPO_ACCESS_TOKEN`.
// Put it behind HTTPS; homeservers post to https://<host>/_matrix/push/v1/notify.

import { configFrom, handle } from './gateway';

declare const Bun: {
  env: Record<string, string | undefined>;
  serve(options: { port: number; fetch(request: Request): Promise<Response> }): { port: number };
};

const config = configFrom(Bun.env);
const server = Bun.serve({ port: Number(Bun.env.PORT ?? 5000), fetch: (request) => handle(request, config) });
console.log(`push gateway listening on port ${server.port} for ${config.appIds.join(', ')}`);
