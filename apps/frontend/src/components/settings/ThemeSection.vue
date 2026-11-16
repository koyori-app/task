<script setup lang="ts">
import { ThemeSwitch } from '@koyori-app/ui-vue';
import { PhInfo } from '@phosphor-icons/vue';
</script>

<template>
  <div class="flex flex-col gap-4" data-theme-section>
    <p class="text-muted-foreground flex items-start gap-2 text-sm" data-device-notice>
      <PhInfo class="mt-0.5 size-4 shrink-0" aria-hidden="true" />
      <span>
        この端末のこのブラウザだけで効く設定です。アカウントには保存されず、ほかの端末や別のブラウザには引き継がれません。
      </span>
    </p>

    <ul class="flex flex-col divide-y rounded-lg border">
      <li class="flex items-center justify-between gap-4 p-4" data-setting="theme">
        <div class="flex min-w-0 flex-col gap-1">
          <span class="text-sm font-medium">ダークテーマ</span>
          <p class="text-muted-foreground text-sm">オンで暗い配色、オフで明るい配色になります。</p>
        </div>
        <ThemeSwitch label="ダークテーマ" class="shrink-0" />
      </li>
    </ul>
  </div>
</template>
