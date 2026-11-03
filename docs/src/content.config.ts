import { defineCollection } from "astro:content";
import { docsCollection, partialsCollection } from "@cloudflare/nimbus-docs/content";

export const collections = {
  docs: defineCollection(
    // 仕様書はコードのコメントから `docs/features/...` のパスで参照されるので、
    // src/content/docs へ移さず docs/ 直下を読む。
    docsCollection({
      base: "../..",
      pattern: "{index.md,breadcrumbs.md,{features,frontend,development}/**/*.md}",
    }),
  ),
  partials: defineCollection(partialsCollection()),
};
