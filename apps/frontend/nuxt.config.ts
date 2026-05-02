import tailwindcss from '@tailwindcss/vite'

// https://nuxt.com/docs/api/configuration/nuxt-config
export default defineNuxtConfig({
  compatibilityDate: "2025-07-15",
  devtools: { enabled: true },
  modules: [
    "@nuxtjs/seo",
    "@pinia/nuxt",
    "@nuxtjs/google-fonts",
    "@vueuse/nuxt"
  ],
  css: ['~/assets/css/tailwind.css'],
  vite: {
    plugins: [
      tailwindcss(),
    ],
  },
  nitro: {
    preset: 'bun'
  }
});