import type { StorybookConfig } from '@storybook/vue3-vite';
import { mergeConfig } from 'vite';

const config: StorybookConfig = {
  stories: ['../stories/**/*.mdx', '../stories/**/*.stories.@(js|jsx|mjs|ts|tsx)'],
  addons: [
    '@chromatic-com/storybook',
    '@storybook/addon-vitest',
    '@storybook/addon-a11y',
    '@storybook/addon-docs',
  ],
  framework: '@storybook/vue3-vite',
  // Storybook には backend が無いので、外部リンクの favicon (/api/v1/favicons/{host}) の
  // 固定の画像だけをここから配る (KFM の story が favicon の描画を撮れるようにする。製品コードは分岐しない)
  staticDirs: ['./static'],
  viteFinal: async (config) =>
    mergeConfig(config, {
      resolve: {
        alias: {
          'vike/client/router': new URL('./mocks/vike-client-router.ts', import.meta.url).pathname,
        },
      },
    }),
};
export default config;
