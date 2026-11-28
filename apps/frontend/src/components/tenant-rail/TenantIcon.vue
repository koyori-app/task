<script setup lang="ts">
import { IconTile } from '@koyori-app/ui-vue';
import { PhBuildings } from '@phosphor-icons/vue';

import type { Tenant } from '@/stores/tenant';

/**
 * テナントのアイコン。画像 → 絵文字 → 頭文字の順と頭文字の取り方は ui の IconTile が持つ。
 * 読み上げ名は包む側が持つ（タイルは読み上げから外れる）。
 */
defineProps<{
  tenant: Pick<Tenant, 'name' | 'icon_url' | 'icon_emoji'> | null;
  size: 24 | 40;
}>();
</script>

<template>
  <IconTile
    v-if="tenant"
    :name="tenant.name"
    :src="tenant.icon_url || undefined"
    :emoji="tenant.icon_emoji ?? undefined"
    :size="size"
  />
  <PhBuildings v-else aria-hidden="true" :size="size" />
</template>
