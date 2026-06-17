<!-- https://vike.dev/Head -->

<template>
  <link rel="icon" :href="logoUrl" />
</template>

<script setup lang="ts">
import '@/assets/css/tailwind.css';
import logoUrl from '../assets/logo.svg';
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
  script: [...umamiScripts],
});
</script>
