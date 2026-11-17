// One local message search, debounced and cancelled as the text changes or its screen closes.

import type { AccountId, MessageHit, RequestId } from '@better-matrix/matrix-native/types';

import { SEARCH_PAUSE_MS } from '../people';
import type { Store } from '../store';

export interface MessageSearch {
  term: string;
  messages: MessageHit[];
  status: 'waiting' | 'searching' | 'ready' | 'failed';
}

export const EMPTY_MESSAGE_SEARCH: MessageSearch = { term: '', messages: [], status: 'ready' };

export class MessageSearcher {
  private timer: ReturnType<typeof setTimeout> | null = null;
  private request: RequestId | null = null;
  private generation = 0;
  private term = '';

  constructor(
    private readonly store: Store,
    private readonly account: AccountId,
    private readonly onChange: (search: MessageSearch) => void,
  ) {}

  type(text: string): void {
    const term = text.trim();

    if (term === this.term) return;
    this.close();
    this.term = term;
    this.onChange({ term, messages: [], status: term === '' ? 'ready' : 'waiting' });

    if (term === '') return;
    const generation = this.generation;
    this.timer = setTimeout(() => {
      this.timer = null;
      this.onChange({ term, messages: [], status: 'searching' });
      let messages: MessageHit[] = [];

      const done = this.store.run(
        { type: 'SearchMessages', data: { account: this.account, term } },
        (event) => {
          if (event.type !== 'MessagesFound' || generation !== this.generation) return;
          messages = event.data.messages;
          this.onChange({ term, messages, status: 'searching' });
        },
      );

      this.request = done.request;

      const finish = (status: 'ready' | 'failed') => {
        if (generation !== this.generation) return;
        this.request = null;
        this.onChange({ term, messages, status });
      };

      done.then(
        () => finish('ready'),
        () => finish('failed'),
      );
    }, SEARCH_PAUSE_MS);
  }

  close(): void {
    this.generation += 1;

    if (this.timer !== null) clearTimeout(this.timer);
    this.timer = null;

    if (this.request !== null)
      this.store.send({ type: 'Cancel', data: this.request }, { quiet: true });
    this.request = null;
  }
}
