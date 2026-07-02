/// <reference types="vitest/config" />
import dotenv from 'dotenv';
import vue from '@vitejs/plugin-vue';
import tailwindcss from '@tailwindcss/vite';
import { sentryVitePlugin } from '@sentry/vite-plugin';
import { analyzer, unstableRolldownAdapter } from 'vite-bundle-analyzer';

/// <reference types="@batijs/core/types" />

import { devtools } from '@tanstack/devtools-vite';
import Inspect from 'vite-plugin-inspect';
import VueDevTools from 'vite-plugin-vue-devtools';
import vike from 'vike/plugin';
import { defineConfig } from 'vite-plus';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { storybookTest } from '@storybook/addon-vitest/vitest-plugin';
import { playwright } from '@vitest/browser-playwright';
import { buildEnv } from './buildSrc/env';
const dirname =
  typeof __dirname !== 'undefined' ? __dirname : path.dirname(fileURLToPath(import.meta.url));

dotenv.config({ path: path.resolve(dirname, '.env') });

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

// More info at: https://storybook.js.org/docs/next/writing-tests/integrations/vitest-addon

export default defineConfig({
  // Standalone build UI (vite build). Embedded client uses +onCreateApp inject.
  plugins: [
    ...(buildEnv.ANALYZE ? [unstableRolldownAdapter(analyzer())] : []),
    ...(buildEnv.VITE_DEVTOOLS ? [devtools()] : []),
    ...(buildEnv.VUE_DEVTOOLS ? [VueDevTools({ appendTo: /\/src\/pages\/\+Layout\.vue/ })] : []),
    ...(buildEnv.VITE_INSPECT ? [Inspect({ build: false })] : []),
    vike(),
    ...(sentryPlugin ? [sentryPlugin] : []),
    tailwindcss(),
    vue(),
  ],
  resolve: {
    alias: {
      '@/sentry.browser.config': path.resolve(dirname, 'sentry.browser.config.ts'),
      '#': path.resolve(dirname, 'server'),
      '@': path.resolve(dirname, 'src'),
    },
  },
  optimizeDeps: {
    include: ['vue', 'reka-ui', '@lucide/vue', '@phosphor-icons/vue'],
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
    noExternal: ['@zxcvbn-ts/core', '@zxcvbn-ts/language-common', '@zxcvbn-ts/language-ja'],
  },
  build: {
    sourcemap: buildEnv.ANALYZE || process.env.NODE_ENV !== 'production',
    rollupOptions: {
      output: {
        manualChunks(id: string) {
          if (id.includes('reka-ui') || id.includes('@floating-ui')) {
            return 'vendor-reka';
          }
        },
      },
    },
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
        'src/components/ui/**',
        'src/components/originui/**',
        'src/generated/**',
      ],
    },
    projects: [
      {
        plugins: [vue()],
        resolve: {
          alias: {
            '@': path.resolve(dirname, 'src'),
          },
        },
        test: {
          name: 'unit',
          environment: 'happy-dom',
          include: ['src/**/*.{test,spec}.{ts,tsx}', 'server/**/*.{test,spec}.{ts,tsx}'],
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
    ignorePatterns: ['content/**/*.md', 'src/components/ui/**', 'src/components/originui/**'],
  },
  lint: {
    plugins: ['oxc', 'typescript', 'unicorn', 'vue'],
    options: { typeAware: true, typeCheck: true },
  },
});
