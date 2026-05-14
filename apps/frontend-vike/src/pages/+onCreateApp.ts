// https://vike.dev/onCreateApp
export { onCreateApp };

import type { PageContext } from "vike/types";
import type { VueHeadClient } from "@unhead/vue";

type PageContextWithUnhead = PageContext & {
  _unhead?: VueHeadClient;
};

async function onCreateApp(pageContext: PageContext) {
  const pageContextWithUnhead = pageContext as PageContextWithUnhead;
  const app = pageContextWithUnhead.app!; //pageContext.app!

  pageContextWithUnhead._unhead ??= await createUnhead();
  app.use(pageContextWithUnhead._unhead);
}

async function createUnhead(): Promise<VueHeadClient> {
  if (import.meta.env.SSR) {
    const { createHead } = await import("@unhead/vue/server");

    // Vike already renders the default charset, viewport, and lang tags.
    return createHead({ disableDefaults: true });
  }

  const { createHead } = await import("@unhead/vue/client");
  return createHead();
}
