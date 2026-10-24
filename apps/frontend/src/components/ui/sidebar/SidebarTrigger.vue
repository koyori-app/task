<script setup lang="ts">
import { PhSidebarSimple } from '@phosphor-icons/vue'
import { Button } from '@koyori-app/ui-vue'
import { useSidebar } from "./utils"

const { toggleSidebar } = useSidebar()
</script>

<template>
  <Button
    data-sidebar="trigger"
    data-slot="sidebar-trigger"
    variant="ghost"
    ariaLabel="サイドバーを開閉"
    @click="toggleSidebar"
  >
    <template #icon>
      <PhSidebarSimple />
    </template>
  </Button>
</template>
