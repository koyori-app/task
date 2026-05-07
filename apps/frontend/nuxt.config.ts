import tailwindcss from '@tailwindcss/vite';
import { config } from './buildSrc/setting';

// https://nuxt.com/docs/api/configuration/nuxt-config
export default defineNuxtConfig({
  compatibilityDate: '2025-07-15',
  devtools: { enabled: true },
  // ブラウザは同一オリジンで /api にアクセスし、Vite がバックエンドへ転送する（localhost:3400 の直叩き回避）
  runtimeConfig: {
    public: {
      apiBase: '/api',
    },
  },
  experimental: {
    typedPages: true,
  },
  future: {
    compatibilityVersion: 5,
  },
  imports: {
    scan: false,
    dirs: [],
  },
  components: {
    dirs: [],
  },
  modules: [
    '@nuxtjs/seo',
    '@pinia/nuxt',
    '@nuxtjs/google-fonts',
    '@vueuse/nuxt',
    'nuxt-umami',
    '@artmizu/nuxt-prometheus',
  ],
  site: {
    defaultLocale: 'ja-JP',
  },
  umami: {
    host: config.UMAMI_HOST,
    id: config.UMAMI_WEBSITE_ID,
  },
  // ref: https://nuxtseo.com/docs/seo-utils/guides/nuxt-config-seo-meta#usage
  seo: {
    meta: {
      charset: 'utf-8',
      applicationName: 'Task',
      // ogp
      ogSiteName: 'Task',
      ogLocale: 'ja_JP',
      ogType: 'website',
      ogUrl: config.APP_URL,
      ogTitle: 'Task',
    },
  },
  css: ['~/assets/css/tailwind.css'],
  vite: {
    plugins: [tailwindcss()],
    server: {
      allowedHosts: true,
      proxy: {
        '/api': {
          target: 'http://127.0.0.1:3400',
          changeOrigin: true,
          rewrite: (path) => path.replace(/^\/api/, ''),
        },
      },
    },
    optimizeDeps: {
      include: [
        'class-variance-authority',
        'clsx',
        'tailwind-merge',
        'lucide-vue-next', // 可能なら廃止したい
        '@phosphor-icons/vue',
        'reka-ui',
        '@tanstack/vue-table'
      ]
    }
  },
  nitro: {
    preset: 'bun',
    compressPublicAssets: true,
  },
  typescript: {
    tsConfig: {
      vueCompilerOptions: {
        checkUnknownComponents: true,
      },
    },
    sharedTsConfig: {},
    nodeTsConfig: {
      include: [
        '../buildSrc/**/*.ts',
        // vite plus only
        '../vite.config.ts',
      ],
    },
  },
});
