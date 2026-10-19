<script setup lang="ts">
import { usePageContext } from 'vike-vue/usePageContext';
import AccountSettingsNav from '@/components/settings/AccountSettingsNav.vue';
import ExperimentsSection from '@/components/settings/ExperimentsSection.vue';

const pageContext = usePageContext();
</script>

<template>
  <div class="mx-auto flex w-full max-w-4xl flex-col gap-6 py-6">
    <header class="flex flex-col gap-1">
      <h1 class="text-2xl font-bold">アカウント設定</h1>
      <p class="text-muted-foreground text-sm">個人アカウントの情報を管理します。</p>
    </header>

    <div class="flex flex-col gap-8 md:flex-row">
      <aside class="md:w-56 md:shrink-0">
        <AccountSettingsNav :current-path="pageContext.urlPathname" />
      </aside>

      <section class="flex min-w-0 flex-1 flex-col gap-6">
        <div class="flex flex-col gap-1">
          <h2 class="text-lg font-semibold">実験的機能</h2>
          <p class="text-muted-foreground text-sm">公開の前の機能を、この端末で先に試せます。</p>
        </div>

        <ExperimentsSection />
      </section>
    </div>
  </div>
</template>
