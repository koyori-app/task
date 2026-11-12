/// <reference types="vitest/config" />
import dotenv from 'dotenv';
import vue from '@vitejs/plugin-vue';
import tailwindcss from '@tailwindcss/vite';
import { sentryVitePlugin } from '@sentry/vite-plugin';
import { analyzer, unstableRolldownAdapter } from 'vite-bundle-analyzer';
import { visualizer } from 'rollup-plugin-visualizer';

/// <reference types="@batijs/core/types" />

import { devtools } from '@tanstack/devtools-vite';
import Inspect from 'vite-plugin-inspect';
import VueDevTools from 'vite-plugin-vue-devtools';
import vike from 'vike/plugin';
import { defineConfig } from 'vite-plus';
import type { Plugin } from 'vite';
import { promises as fs } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { zstdCompressSync } from 'node:zlib';
import { storybookTest } from '@storybook/addon-vitest/vitest-plugin';
import { playwright } from '@vitest/browser-playwright';
import { buildEnv } from './buildSrc/env';
const dirname =
  typeof __dirname !== 'undefined' ? __dirname : path.dirname(fileURLToPath(import.meta.url));
// Vue 3.6 RC's default bundler entry re-exports Vapor; this VDOM-only app does not use it.
const vueRuntimeDom = fileURLToPath(
  import.meta.resolve('@vue/runtime-dom/dist/runtime-dom.esm-bundler.js'),
);

dotenv.config({ path: path.resolve(dirname, '.env'), quiet: true });

const coderAllowedHost = buildEnv.CODER_AGENT_URL
  ? `.${new URL(buildEnv.CODER_AGENT_URL).hostname}`
  : undefined;

const sentryPlugin =
  process.env.NODE_ENV?.includes('prod') || buildEnv.FORCE_ENABLE_IN_DEV
    ? sentryVitePlugin({
        sourcemaps: {
          disable: false,
        },
      })
    : undefined;

function getBundleVisualizerPlugins() {
  if (process.env.FRONTEND_BUNDLE_VISUALIZER !== 'true') return [];

  const projectRoot = path.resolve(dirname, '../..');
  const statsFilename = process.env.FRONTEND_BUNDLE_VISUALIZER_FILE ?? 'bundle-stats.json';
  const commonOptions = {
    title: 'Task frontend bundle visualizer',
    gzipSize: true,
    brotliSize: true,
    projectRoot,
  };

  type RawReport = {
    nodeParts: Record<string, { zstdLength?: number }>;
    nodeMetas: Record<string, { id: string; moduleParts: Record<string, string> }>;
    options: Record<string, boolean>;
  };

  const zstdPlugin: Plugin = {
    name: 'frontend-bundle-diagnostics-zstd',
    async generateBundle(_outputOptions, outputBundle) {
      const statsPath = path.resolve(dirname, statsFilename);
      let raw: string;
      try {
        raw = await fs.readFile(statsPath, 'utf8');
      } catch (error) {
        const code = (error as NodeJS.ErrnoException).code;
        if (code === 'ENOENT') {
          throw new Error(
            `frontend-bundle-diagnostics: missing ${statsFilename}; rollup-plugin-visualizer must emit stats.json before the zstd plugin runs`,
          );
        }
        throw error;
      }
      const report = JSON.parse(raw) as RawReport;
      const metasById = new Map(Object.values(report.nodeMetas).map((meta) => [meta.id, meta]));

      for (const [bundleId, bundle] of Object.entries(outputBundle)) {
        if (bundle.type !== 'chunk') continue;
        for (const [moduleId, module] of Object.entries(bundle.modules)) {
          const reportId = moduleId.startsWith(projectRoot)
            ? moduleId.slice(projectRoot.length)
            : moduleId.replace(projectRoot, '');
          const partId = metasById.get(reportId)?.moduleParts[bundleId];
          if (!partId) continue;
          report.nodeParts[partId].zstdLength = module.code
            ? zstdCompressSync(Buffer.from(module.code, 'utf8')).byteLength
            : 0;
        }
      }

      report.options.zstd = true;
      await fs.writeFile(statsPath, JSON.stringify(report));
    },
  };
  const plugins = [
    Object.assign(
      visualizer({
        ...commonOptions,
        filename: statsFilename,
        template: 'raw-data',
      }),
      { applyToEnvironment: (environment: { name: string }) => environment.name === 'client' },
    ),
    Object.assign(zstdPlugin, {
      applyToEnvironment: (environment: { name: string }) => environment.name === 'client',
    }),
  ];

  if (process.env.FRONTEND_BUNDLE_VISUALIZER_HTML_FILE) {
    plugins.push(
      Object.assign(
        visualizer({
          ...commonOptions,
          filename: process.env.FRONTEND_BUNDLE_VISUALIZER_HTML_FILE,
          template: 'treemap',
        }),
        { applyToEnvironment: (environment: { name: string }) => environment.name === 'client' },
      ),
    );
  }

  return plugins;
}

// More info at: https://storybook.js.org/docs/next/writing-tests/integrations/vitest-addon

// SSR の依存を dist/server に同梱すると、tr46 の require('punycode/')（npm の punycode を指す書き方）が
// バンドル後に node:punycode/ になり、bun が「そんな組み込みは無い」と落ちる。
// 組み込みの node:punycode で同じ API が使えるので、出力の文字列だけ直す。
function punycodeBuiltinPlugin() {
  return {
    name: 'punycode-builtin',
    renderChunk(code: string) {
      return code.includes('"node:punycode/"')
        ? code.replaceAll('"node:punycode/"', '"node:punycode"')
        : null;
    },
  };
}

export default defineConfig(({ command }) => ({
  // Standalone build UI (vite build). Embedded client uses +onCreateApp inject.
  plugins: [
    ...getBundleVisualizerPlugins(),
    ...(buildEnv.ANALYZE ? [unstableRolldownAdapter(analyzer())] : []),
    ...(buildEnv.VITE_DEVTOOLS ? [devtools()] : []),
    ...(buildEnv.VUE_DEVTOOLS ? [VueDevTools({ appendTo: /\/src\/pages\/\+Layout\.vue/ })] : []),
    ...(buildEnv.VITE_INSPECT ? [Inspect({ build: false })] : []),
    vike(),
    punycodeBuiltinPlugin(),
    ...(sentryPlugin ? [sentryPlugin] : []),
    tailwindcss(),
    vue(),
  ],
  resolve: {
    alias: [
      { find: /^vue$/, replacement: vueRuntimeDom },
      {
        find: '@/sentry.browser.config',
        replacement: path.resolve(dirname, 'sentry.browser.config.ts'),
      },
      { find: '#', replacement: path.resolve(dirname, 'server') },
      { find: '@', replacement: path.resolve(dirname, 'src') },
    ],
  },
  optimizeDeps: {
    include: ['vue', '@lucide/vue', '@phosphor-icons/vue'],
  },
  server: {
    proxy: {
      '/api': {
        target: buildEnv.API_BASE ?? 'http://localhost:3400',
        changeOrigin: true,
        rewrite: (path) => path.replace(/^\/api/, ''),
      },
    },
    warmup: {
      clientFiles: [
        './src/pages/+Layout.vue',
        // './src/pages/index/+Page.vue'
      ],
    },
    allowedHosts: ['localhost', '127.0.0.1', ...(coderAllowedHost ? [coderAllowedHost] : [])],
  },
  ssr: {
    // SSR の依存を dist/server に同梱し、配布イメージに node_modules を積まずに済ませる
    // （apps/frontend/Dockerfile）。dev の module runner は CommonJS の依存をそのまま実行できず
    // SSR が落ちるので、全部の同梱は build だけにし、dev は以前から同梱していた @zxcvbn-ts に留める
    noExternal:
      command === 'build'
        ? true
        : ['@zxcvbn-ts/core', '@zxcvbn-ts/language-common', '@zxcvbn-ts/language-ja'],
    // jsdom は __dirname 相対で default-stylesheet.css 等を読むので同梱できない。
    // isomorphic-dompurify ごと外部に残し、Dockerfile がその閉包だけを node_modules に置く。
    // starry-night は実行時に vscode-oniguruma の onig.wasm を node_modules から読むので、同じく外部に残す
    external: ['isomorphic-dompurify', '@wooorm/starry-night'],
  },
  build: {
    sourcemap: buildEnv.ANALYZE || process.env.NODE_ENV !== 'production',
  },
  test: {
    maxWorkers: 4,
    coverage: {
      provider: 'v8',
      reporter: ['lcov', 'text', 'json-summary', 'json'],
      reportsDirectory: './coverage',
      reportOnFailure: true,
      exclude: [
        '**/*.stories.{ts,tsx,js,jsx}',
        '**/*.story.{ts,tsx,js,jsx}',
        '**/.storybook/**',
        'storybook-static/**',
        'src/generated/**',
      ],
    },
    projects: [
      {
        // vitest 5 から inline の project は root の設定を受け継ぐ（extends の既定が true になった）。
        // unit は happy-dom で組むため、root の vike や tailwind の plugin を持ち込まず、これまでどおり
        // 自前の plugin と alias だけで組む
        extends: false,
        plugins: [vue()],
        resolve: {
          alias: {
            '@/sentry.browser.config': path.resolve(dirname, 'sentry.browser.config.ts'),
            '#': path.resolve(dirname, 'server'),
            '@': path.resolve(dirname, 'src'),
          },
        },
        test: {
          name: 'unit',
          environment: 'happy-dom',
          include: [
            'src/**/*.{test,spec}.{ts,tsx}',
            'server/**/*.{test,spec}.{ts,tsx}',
            'oxlint-plugins/**/*.{test,spec}.mjs',
            '../../.github/scripts/**/*.{test,spec}.mts',
          ],
          globals: true,
        },
      },
      {
        extends: true,
        plugins: [
          // The plugin will run tests for the stories defined in your Storybook config
          // See options at: https://storybook.js.org/docs/next/writing-tests/integrations/vitest-addon#storybooktest
          storybookTest({
            configDir: path.join(dirname, '.storybook'),
          }),
        ],
        test: {
          name: 'storybook',
          browser: {
            enabled: true,
            headless: true,
            provider: playwright({}),
            instances: [
              {
                browser: 'chromium',
              },
            ],
          },
        },
      },
    ],
  },
  fmt: {
    singleQuote: true,
    trailingComma: 'all',
    ignorePatterns: [
      'content/**/*.md',
      // renderDescription の事前生成 fixture (toMatchFileSnapshot)。整形すると
      // レンダラ出力とのバイト一致が壊れ drift 検査が偽陽性で落ちる
      'src/lib/kfm-story-fixtures/rendered/**',
    ],
  },
  lint: {
    plugins: ['oxc', 'typescript', 'unicorn', 'vue'],
    jsPlugins: [
      '@koyori-app/oxlint-plugin-api-path-params',
      path.resolve(dirname, 'oxlint-plugins/no-date-now.mjs'),
    ],
    rules: {
      'api-path-params/no-raw-route-id-in-api-path': 'error',
    },
    overrides: [
      {
        files: ['src/**/*.{ts,tsx,vue}'],
        rules: {
          'no-date-now/no-date-now': 'error',
        },
      },
    ],
    options: { typeAware: true, typeCheck: true },
  },
}));
