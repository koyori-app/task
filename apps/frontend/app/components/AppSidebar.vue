<script setup lang="ts">
import type { SidebarProps } from '@/components/ui/sidebar';

import {
  BookOpen,
  Bot,
  Settings2,
  SquareTerminal,
} from 'lucide-vue-next';
import { ref, onMounted, computed } from 'vue';
import NavMain from '@/components/NavMain.vue';
import NavProjects from '@/components/NavProjects.vue';
import NavUser from '@/components/NavUser.vue';
import TenantSwitcher from '@/components/TenantSwitcher.vue';

import {
  Sidebar,
  SidebarContent,
  SidebarFooter,
  SidebarHeader,
  SidebarRail,
} from '@/components/ui/sidebar';
import { useTenantApi, type Tenant } from '@/composables/useTenantApi';
import { useProjectApi, type Project } from '@/composables/useProjectApi';
import { useAuthApi } from '@/composables/useAuthApi';

const props = withDefaults(defineProps<SidebarProps>(), {
  collapsible: 'icon',
});

const navMain = [
  {
    title: 'Labels',
    url: '/labels',
    icon: SquareTerminal,
    isActive: true,
  },
  {
    title: 'Documentation',
    url: '#',
    icon: BookOpen,
    items: [
      { title: 'Introduction', url: '#' },
      { title: 'Get Started', url: '#' },
    ],
  },
  {
    title: 'Settings',
    url: '#',
    icon: Settings2,
    items: [
      { title: 'General', url: '#' },
      { title: 'Team', url: '#' },
      { title: 'Billing', url: '#' },
    ],
  },
];

const currentUser = ref<{ name: string; email: string; avatar: string } | null>(null);
const tenants = ref<Tenant[]>([]);
const activeTenantId = ref<string | null>(null);
const projects = ref<Project[]>([]);

const tenantItems = computed(() =>
  tenants.value.map((t) => ({ id: t.id, name: t.name, display_id: t.display_id })),
);

// TODO: project-detail ルート（app/pages）定義後に NavProjects を有効化し、
// NuxtLink で { name: 'project-detail', params: { projectId } } 等の実ルートを渡す
const projectItems = computed(() =>
  projects.value.map((p) => ({ name: p.name, url: '', id: p.id })),
);

async function loadProjects(tenantId: string) {
  try {
    const api = useProjectApi(tenantId);
    projects.value = await api.list();
  } catch {
    projects.value = [];
  }
}

onMounted(async () => {
  const authApi = useAuthApi();
  const tenantApi = useTenantApi();

  try {
    const user = await authApi.getCurrentUser();
    currentUser.value = { name: user.username, email: user.email, avatar: '' };
  } catch {
    // 未ログイン時はデフォルト表示のまま
  }

  try {
    tenants.value = await tenantApi.list();
    if (tenants.value.length > 0) {
      activeTenantId.value = tenants.value[0]!.id;
      await loadProjects(activeTenantId.value);
    }
  } catch {
    // テナント取得失敗時はプロジェクト一覧も空のまま
  }
});

async function onTenantChange(tenantId: string) {
  activeTenantId.value = tenantId;
  await loadProjects(tenantId);
}
</script>

<template>
  <Sidebar v-bind="props">
    <SidebarHeader>
      <TenantSwitcher :tenants="tenantItems" @change="onTenantChange" />
    </SidebarHeader>
    <SidebarContent>
      <NavMain :items="navMain" />
      <!-- TODO: プロジェクト詳細ページのルート未定義のため NavProjects を無効化（'#' プレースホルダー回避） -->
      <!-- <NavProjects :projects="projectItems" /> -->
    </SidebarContent>
    <SidebarFooter>
      <NavUser
        :user="currentUser ?? { name: '—', email: '', avatar: '' }"
      />
    </SidebarFooter>
    <SidebarRail />
  </Sidebar>
</template>
