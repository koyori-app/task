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
  compatibilityDate: '2024-07-06',
});
