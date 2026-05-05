// https://nuxt.com/docs/api/configuration/nuxt-config
export default defineNuxtConfig({
  site: {
    url: 'https://docs.tasks.akarinext.org'
  },
  devtools: { enabled: true },
  modules: ['nuxt-component-meta'],
  extends: ['shadcn-docs-nuxt'],
  i18n: {
    defaultLocale: 'en',
    locales: [
      {
        code: 'en',
        name: 'English',
        language: 'en-US',
      },
    ],
  },
    vite: {
    optimizeDeps: {
      include: [
        'debug', // CJS
        'mermaid',
        '@vue/devtools-core',
        '@vue/devtools-kit',
      ]
    }
  },
  compatibilityDate: '2024-07-06',
  nitro: {
    prerender: {
      // Don't fail the whole generate process on prerender errors
      failOnError: false,
    },
  },
});
