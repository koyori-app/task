import { defineCollection } from "astro:content";
import { z } from "astro/zod";
import { docsCollection, partialsCollection } from "@cloudflare/nimbus-docs/content";

export const collections = {
  docs: defineCollection(
    // 仕様書はコードのコメントから `docs/features/...` のパスで参照されるので、
    // src/content/docs へ移さず docs/ 直下を読む。
    docsCollection({
      base: "../..",
      pattern: "{index.md,breadcrumbs.md,{features,frontend,development}/**/*.md}",
      // 仕様のページの欄（docs/STYLE.md）。検査（scripts/lint-style.mjs）と同じ値だけを許す。
      // 新しい形へ移していないページは持たないので、どちらも任意の欄にする。
      schemaFields: {
        status: z.enum(["implemented", "partial", "planned", "operational"]).optional(),
        updated: z.coerce.date().optional(),
      },
    }),
  ),
  partials: defineCollection(partialsCollection()),
};
