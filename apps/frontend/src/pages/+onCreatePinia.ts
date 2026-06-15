export { onCreatePinia };

import { createPersistedState } from 'pinia-plugin-persistedstate';
import type { Pinia } from 'pinia';
import type { PageContext } from 'vike/types';

type PageContextWithPinia = PageContext & {
  pinia?: Pinia;
};

function onCreatePinia(pageContext: PageContextWithPinia) {
  if (import.meta.env.SSR) return;
  pageContext.pinia?.use(
    createPersistedState({ storage: localStorage }),
  );
}
