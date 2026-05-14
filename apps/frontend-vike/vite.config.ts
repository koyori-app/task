/// <reference types="vitest/config" />
import vue from '@vitejs/plugin-vue';
import tailwindcss from '@tailwindcss/vite';
import { sentryVitePlugin } from '@sentry/vite-plugin';
/// <reference types="@batijs/core/types" />

import vike from 'vike/plugin';
import { defineConfig } from 'vite-plus';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { storybookTest } from '@storybook/addon-vitest/vitest-plugin';
import { playwright } from '@vitest/browser-playwright';
const dirname =
  typeof __dirname !== 'undefined' ? __dirname : path.dirname(fileURLToPath(import.meta.url));

// More info at: https://storybook.js.org/docs/next/writing-tests/integrations/vitest-addon
export default defineConfig({
  plugins: [
    vike(),
    sentryVitePlugin({
      sourcemaps: {
        disable: false,
      },
    }),
    tailwindcss(),
    vue(),
  ],
  resolve: {
    alias: {
      '#': path.resolve(dirname, 'server'),
      '@/assets': path.resolve(dirname, 'src/assets'),
      '@/components': path.resolve(dirname, 'src/components'),
      '@/pages': path.resolve(dirname, 'src/pages'),
      '@/sentry.browser.config': path.resolve(dirname, 'sentry.browser.config.ts'),
    },
  },
  build: {
    sourcemap: true,
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
