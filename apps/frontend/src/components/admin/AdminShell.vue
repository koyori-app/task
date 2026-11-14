<script setup lang="ts">
import { usePageContext } from 'vike-vue/usePageContext';
import { useMeQuery } from '@/lib/api-vue-query';

defineProps<{ title: string; description?: string }>();

const pageContext = usePageContext();
const meQuery = useMeQuery();

const links = [
  { href: '/admin/users', label: 'ユーザー' },
  { href: '/admin/settings', label: 'システム設定' },
];
</script>

<template>
  <div class="mx-auto flex w-full max-w-4xl flex-col gap-6 py-6">
    <!-- 親レイアウトが /me の成功後だけ描画する。管理者でなければ中身は描かない（API も 403 で拒否する）。 -->
    <p v-if="!meQuery.data.value?.is_admin" role="alert" class="text-destructive text-sm">
      このページは管理者だけが利用できます。
    </p>
    <template v-else>
      <header class="flex flex-col gap-1">
        <h1 class="text-2xl font-bold">{{ title }}</h1>
        <p v-if="description" class="text-muted-foreground text-sm">{{ description }}</p>
      </header>
      <nav class="flex gap-1" aria-label="管理">
        <a
          v-for="link in links"
          :key="link.href"
          :href="link.href"
          :aria-current="pageContext.urlPathname.startsWith(link.href) ? 'page' : undefined"
          class="rounded-md px-3 py-2 text-sm aria-[current=page]:bg-accent aria-[current=page]:font-medium hover:bg-accent/50"
        >
          {{ link.label }}
        </a>
      </nav>
      <slot />
    </template>
  </div>
</template>
