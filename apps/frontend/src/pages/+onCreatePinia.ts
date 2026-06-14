export { onCreatePinia };

import { createPersistedState } from 'pinia-plugin-persistedstate';
import type { PageContext } from 'vike/types';

function onCreatePinia(pageContext: PageContext) {
  if (import.meta.env.SSR) return;
  (pageContext as any).pinia?.use(
    createPersistedState({ storage: localStorage }),
  );
}
