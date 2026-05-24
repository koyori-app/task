/// <reference types="vitest/config" />
import dotenv from 'dotenv';
import vue from '@vitejs/plugin-vue';
import tailwindcss from '@tailwindcss/vite';
import { sentryVitePlugin } from '@sentry/vite-plugin';
import { analyzer, unstableRolldownAdapter } from 'vite-bundle-analyzer'

const analyze = process.env.ANALYZE === 'true';
/// <reference types="@batijs/core/types" />

import vike from 'vike/plugin';
import { defineConfig } from 'vite-plus';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { storybookTest } from '@storybook/addon-vitest/vitest-plugin';
import { playwright } from '@vitest/browser-playwright';
const dirname =
  typeof __dirname !== 'undefined' ? __dirname : path.dirname(fileURLToPath(import.meta.url));

dotenv.config({ path: path.resolve(dirname, '.env') });

const sentryEnabled =
  process.env.NODE_ENV?.includes('prod') ||
  process.env.FORCE_ENABLE_IN_DEV === 'true';

// console.log('FORCE_ENABLE_IN_DEV: ', process.env.FORCE_ENABLE_IN_DEV === 'true');
// console.log('sentryEnabled: ', sentryEnabled);
const sentryPlugin = sentryEnabled
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
    ...(analyze ? [unstableRolldownAdapter(analyzer())] : []),
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
    include: ['vue', 'reka-ui', 'lucide-vue-next', '@phosphor-icons/vue'],
  },
  server: {
    warmup: {
      clientFiles: [
        './src/pages/+Layout.vue',
        // './src/pages/index/+Page.vue'
        ],
    },
  },
  build: {
    sourcemap: analyze || process.env.NODE_ENV !== 'production',
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
    projects: [
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
    ignorePatterns: ['content/**/*.md'],
  },
  lint: { options: { typeAware: true, typeCheck: true } },
});
