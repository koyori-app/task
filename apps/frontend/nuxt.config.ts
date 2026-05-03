import tailwindcss from '@tailwindcss/vite'

// https://nuxt.com/docs/api/configuration/nuxt-config
export default defineNuxtConfig({
  compatibilityDate: "2025-07-15",
  devtools: { enabled: true },
  modules: [
    "@nuxtjs/seo",
    "@pinia/nuxt",
    "@nuxtjs/google-fonts",
    "@vueuse/nuxt",
    "nuxt-umami",
    "@artmizu/nuxt-prometheus"
  ],
  css: ['~/assets/css/tailwind.css'],
  vite: {
    plugins: [
      tailwindcss(),
    ],
    server: {
      allowedHosts: true
    }
  },
  nitro: {
    preset: 'bun'
  }
});