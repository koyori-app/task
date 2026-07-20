import type { StorybookConfig } from '@storybook/vue3-vite';
import { mergeConfig } from 'vite';

const config: StorybookConfig = {
  stories: ['../stories/**/*.mdx', '../stories/**/*.stories.@(js|jsx|mjs|ts|tsx)'],
  addons: ['@chromatic-com/storybook', '@storybook/addon-a11y', '@storybook/addon-docs'],
  framework: '@storybook/vue3-vite',
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
