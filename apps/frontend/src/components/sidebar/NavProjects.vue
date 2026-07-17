<script setup lang="ts">
import {
  PhDotsThree,
  PhFolderOpen,
  PhPencilSimple,
  PhPlus,
  PhTrash,
  PhUser,
} from '@phosphor-icons/vue';
import { computed } from 'vue';
import type { components } from '@/generated/api';

import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import {
  SidebarGroup,
  SidebarGroupAction,
  SidebarGroupLabel,
  SidebarMenu,
  SidebarMenuAction,
  SidebarMenuButton,
  SidebarMenuItem,
  useSidebar,
} from '@/components/ui/sidebar';

export type ProjectNavItem = components['schemas']['ProjectResponse'];

const props = defineProps<{
  tenantSlug: string;
  projects: components['schemas']['ProjectResponse'][];
  loading?: boolean;
  error?: boolean;
}>();

const emit = defineEmits<{
  retry: [];
  create: [];
  edit: [project: ProjectNavItem];
  delete: [project: ProjectNavItem];
}>();

const { isMobile } = useSidebar();

const sortedProjects = computed(() =>
  [...props.projects].sort((a, b) => {
    if (a.is_personal === b.is_personal) return a.name.localeCompare(b.name);
    return a.is_personal ? -1 : 1;
  }),
);

function projectTasksUrl(project: components['schemas']['ProjectResponse']) {
  return `/${props.tenantSlug}/projects/${project.key}/tasks`;
}
</script>

<template>
  <SidebarGroup class="group-data-[collapsible=icon]:hidden">
    <SidebarGroupLabel>Projects</SidebarGroupLabel>
    <SidebarGroupAction
      v-if="tenantSlug"
      as="button"
      type="button"
      title="プロジェクトを作成"
      @click="emit('create')"
    >
      <PhPlus />
      <span class="sr-only">プロジェクトを作成</span>
    </SidebarGroupAction>
    <SidebarMenu v-if="loading">
      <SidebarMenuItem>
        <SidebarMenuButton disabled>
          <span class="text-muted-foreground text-sm">プロジェクトを読み込み中…</span>
        </SidebarMenuButton>
      </SidebarMenuItem>
    </SidebarMenu>
    <SidebarMenu v-else-if="error">
      <SidebarMenuItem>
        <SidebarMenuButton class="text-destructive" @click="emit('retry')">
          プロジェクト一覧を取得できませんでした（再試行）
        </SidebarMenuButton>
      </SidebarMenuItem>
    </SidebarMenu>
    <SidebarMenu v-else-if="sortedProjects.length === 0">
      <SidebarMenuItem>
        <SidebarMenuButton disabled>
          <span class="text-muted-foreground text-sm">プロジェクトがありません</span>
        </SidebarMenuButton>
      </SidebarMenuItem>
    </SidebarMenu>
    <SidebarMenu v-else>
      <SidebarMenuItem v-for="project in sortedProjects" :key="project.id">
        <SidebarMenuButton v-if="tenantSlug" as-child>
          <a :href="projectTasksUrl(project)">
            <img
              v-if="project.icon_url"
              :src="project.icon_url"
              :alt="project.name"
              class="size-4 shrink-0 rounded-sm object-cover"
            />
            <span v-else-if="project.icon_emoji" class="text-base leading-none">{{
              project.icon_emoji
            }}</span>
            <PhUser v-else-if="project.is_personal" />
            <PhFolderOpen v-else />
            <span>{{ project.name }}</span>
          </a>
        </SidebarMenuButton>
        <SidebarMenuButton v-else>
          <img
            v-if="project.icon_url"
            :src="project.icon_url"
            :alt="project.name"
            class="size-4 shrink-0 rounded-sm object-cover"
          />
          <span v-else-if="project.icon_emoji" class="text-base leading-none">{{
            project.icon_emoji
          }}</span>
          <PhUser v-else-if="project.is_personal" />
          <PhFolderOpen v-else />
          <span>{{ project.name }}</span>
        </SidebarMenuButton>
        <DropdownMenu v-if="!project.is_personal">
          <DropdownMenuTrigger as-child>
            <SidebarMenuAction show-on-hover :aria-label="`${project.name} の操作`">
              <PhDotsThree />
              <span class="sr-only">More</span>
            </SidebarMenuAction>
          </DropdownMenuTrigger>
          <DropdownMenuContent
            class="w-48 rounded-lg"
            :side="isMobile ? 'bottom' : 'right'"
            :align="isMobile ? 'end' : 'start'"
          >
            <DropdownMenuItem @select="emit('edit', project)">
              <PhPencilSimple class="text-muted-foreground" />
              <span>編集</span>
            </DropdownMenuItem>
            <DropdownMenuSeparator />
            <DropdownMenuItem variant="destructive" @select="emit('delete', project)">
              <PhTrash />
              <span>削除</span>
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </SidebarMenuItem>
    </SidebarMenu>
  </SidebarGroup>
</template>
