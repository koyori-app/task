import { defineConfig } from "astro/config";
import tailwindcss from "@tailwindcss/vite";
import nimbus, {
  defineConfig as defineNimbusConfig,
} from "@cloudflare/nimbus-docs";
import { tableScroll } from "@cloudflare/nimbus-docs/markdown";
import { mermaidFence } from "./src/lib/mermaid-fence";

const nimbusConfig = defineNimbusConfig({
  site: "https://docs.koyori.app",
  title: "Koyori Docs",
  description: "Koyori の仕様書と開発ドキュメント",
  locale: "ja",
  github: "https://github.com/koyori-app/task",
  socialImageAlt: "Koyori Docs",
  // The home page leads the sidebar; every other page joins it automatically.
  sidebar: {
    items: ["index", { autogenerate: { collection: "docs" } }],
  },
});

export default defineConfig({
  // nimbus:adapter
  output: "static",
  // 番号付きの旧 URL を保つ（slug で番号を外したページ）
  redirects: {
    "/features/tasks/1.core": "/features/tasks/core/",
    "/features/tasks/10.webhooks": "/features/tasks/webhooks/",
    "/features/tasks/11.schedule": "/features/tasks/schedule/",
    "/features/tasks/12.workload": "/features/tasks/workload/",
    "/features/tasks/2.time-tracking": "/features/tasks/time-tracking/",
    "/features/tasks/3.sprints": "/features/tasks/sprints/",
    "/features/tasks/4.collaboration-clipboard": "/features/tasks/collaboration-clipboard/",
    "/features/tasks/4.collaboration": "/features/tasks/collaboration/",
    "/features/tasks/5.notifications": "/features/tasks/notifications/",
    "/features/tasks/6.custom-fields": "/features/tasks/custom-fields/",
    "/features/tasks/7.extensions": "/features/tasks/extensions/",
    "/features/tasks/8.automation": "/features/tasks/automation/",
    "/features/tasks/9.github-app": "/features/tasks/github-app/",
    "/features/tasks/9.github-tasks": "/features/tasks/github-tasks/",
  },
  // Tailwind v4 via its Vite plugin (the integration Astro recommends for
  // Tailwind v4 — replaces the PostCSS plugin, which doesn't build under
  // Astro 7's Vite 8 bundler).
  vite: {
    plugins: [tailwindcss()],
  },
  // Hover-prefetch link targets so full-page navigations feel instant without
  // a client-side router.
  prefetch: {
    prefetchAll: true,
    defaultStrategy: "hover",
  },
  integrations: [
    nimbus(nimbusConfig, {
      // Authoring rules are opt-in by design — your repo, your taste. The
      // two below are the load-bearing pair: frontmatter has to validate
      // against the content schema for the page to render properly, and
      // broken internal links are 404s for your readers. Add the others
      // (heading hierarchy, code-block language, style, etc.) when you're
      // ready to enforce them — see `nimbus-docs lint --help`.
      rules: {
        "nimbus/frontmatter-shape": "error",
        "nimbus/internal-link": "error",
      },
      markdown: {
        // Wrap wide tables so they scroll instead of overflowing the page
        // (styled by `.nb-table-scroll` in src/styles/prose.css).
        hastPlugins: [tableScroll(), mermaidFence()],
      },
    }),
  ],
});
