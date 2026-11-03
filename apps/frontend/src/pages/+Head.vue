<!-- https://vike.dev/Head -->

<template>
  <link
    v-for="favicon in FAVICONS"
    :key="favicon.href"
    rel="icon"
    type="image/png"
    :sizes="favicon.sizes"
    :href="favicon.href"
  />
  <link rel="manifest" :href="MANIFEST_PATH" />
  <meta name="theme-color" :content="PWA_THEME_COLOR" />
  <link rel="apple-touch-icon" :href="PWA_ICONS.appleTouch" />
  <meta name="apple-mobile-web-app-title" content="task" />
</template>

<script setup lang="ts">
import '@/assets/css/tailwind.css';
import { emojiFontBootScript } from '@/lib/emoji-font';
import { FAVICONS, MANIFEST_PATH, PWA_ICONS, PWA_THEME_COLOR } from '@/lib/pwa/app-manifest';
import { usePreferencesStore } from '@/stores/preferences';
import { useHead } from '@unhead/vue';
import { usePageContext } from 'vike-vue/usePageContext';

type PageContextWithSettings = ReturnType<typeof usePageContext> & {
  settings: {
    env: {
      UMAMI_HOST?: string;
      UMAMI_WEBSITE_ID?: string;
    };
  };
};

const { UMAMI_HOST, UMAMI_WEBSITE_ID } = (usePageContext() as PageContextWithSettings).settings.env;
const umamiHost = UMAMI_HOST?.replace(/\/$/, '');
const umamiScripts =
  umamiHost && UMAMI_WEBSITE_ID
    ? [
        {
          defer: true,
          src: `${umamiHost}/script.js`,
          'data-website-id': UMAMI_WEBSITE_ID,
        },
        {
          defer: true,
          src: `${umamiHost}/recorder.js`,
          'data-website-id': UMAMI_WEBSITE_ID,
          'data-sample-rate': '0.15',
          'data-mask-level': 'moderate',
          'data-max-duration': '300000',
        },
      ]
    : [];

useHead({
  link: [
    { rel: 'preconnect', href: 'https://fonts.googleapis.com' },
    { rel: 'preconnect', href: 'https://fonts.gstatic.com', crossorigin: '' },
  ],
  script: [
    // 環境設定で選んだ絵文字の字を、body を描く前に html へ当てる（最初の一瞬の入れ替わりを防ぐ）
    {
      key: 'emoji-font',
      innerHTML: emojiFontBootScript(usePreferencesStore.$id),
      tagPriority: 'critical',
    },
    ...umamiScripts,
  ],
});
</script>
