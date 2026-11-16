// The gateway as a Cloudflare Worker (`wrangler deploy src/worker.ts`); `APP_IDS` and
// `EXPO_ACCESS_TOKEN` come from the Worker's variables and secrets.

import { configFrom, handle } from './gateway';

export default {
  fetch(request: Request, env: Record<string, string | undefined>): Promise<Response> {
    return handle(request, configFrom(env));
  },
};
