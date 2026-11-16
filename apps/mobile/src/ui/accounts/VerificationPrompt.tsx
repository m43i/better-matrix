// Opens the verification sheet when another of the user's sessions asks to verify, for any
// signed-in account. Owned by the accounts area; mounted at the root.

import { router } from 'expo-router';
import { useEffect, useState } from 'react';

import { routes } from '../../routes';
import { newIncoming, verificationKey } from '../../state/actions/security';
import { useAppStore, useStore } from '../../state/react';

export function VerificationPrompt() {
  const store = useAppStore();
  const verifications = useStore((s) => s.verifications);
  // Requests that were there already are not asked again.
  const [seen] = useState(() => new Set(store.getState().verifications.map(verificationKey)));
  useEffect(() => {
    for (const v of newIncoming(verifications, seen)) {
      seen.add(verificationKey(v));
      router.push(routes.verify(v.account, v.flowId));
    }
  }, [verifications, seen]);

  return null;
}
