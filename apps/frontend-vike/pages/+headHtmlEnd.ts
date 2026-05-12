export { headHtmlEnd };

import type { UseHeadInput, VueHeadClient } from "@unhead/vue";
import type { SSRHeadPayload } from "@unhead/vue/server";
import type { PageContextServer } from "vike/types";

type PageContextWithUnhead = PageContextServer & {
  _unhead?: VueHeadClient<UseHeadInput, SSRHeadPayload>;
};

function headHtmlEnd(pageContext: PageContextServer) {
  const head = (pageContext as PageContextWithUnhead)._unhead;
  if (!head) return "";

  return head.render().headTags;
}
