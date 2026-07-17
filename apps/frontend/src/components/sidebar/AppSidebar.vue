<script setup lang="ts">
import type { SidebarProps } from '@/components/ui/sidebar';
import { useAuthSession } from '@/composables/useAuthSession';
import { useRouteAlignedTenantId } from '@/composables/useRouteAlignedTenantId';
import { useAuthStore } from '@/stores/auth';
import { useTenantStore, type Tenant } from '@/stores/tenant';
import { useProjectsQuery } from '@/lib/api-vue-query';
import { usePageContext } from 'vike-vue/usePageContext';
import { navigate } from 'vike/client/router';
import { computed, ref, watch } from 'vue';
import type { components } from '@/generated/api';

import { BookOpen, Bot, ListTodo, Settings2, SquareTerminal } from '@lucide/vue';
import DeleteProjectDialog from '@/components/sidebar/DeleteProjectDialog.vue';
import NavMain from '@/components/sidebar/NavMain.vue';
import NavProjects from '@/components/sidebar/NavProjects.vue';
import NavUser from '@/components/sidebar/NavUser.vue';
import ProjectFormDialog from '@/components/sidebar/ProjectFormDialog.vue';
import TenantSwitcher from '@/components/sidebar/TenantSwitcher.vue';

import {
  Sidebar,
  SidebarContent,
  SidebarFooter,
  SidebarHeader,
  SidebarRail,
} from '@/components/ui/sidebar';

const props = withDefaults(defineProps<SidebarProps>(), {
  collapsible: 'icon',
});

const pageContext = usePageContext();
const authStore = useAuthStore();
const tenantStore = useTenantStore();
const { logout } = useAuthSession();

const tenantSlug = computed(() => {
  const { tenant } = pageContext.routeParams;
  return typeof tenant === 'string' ? tenant : '';
});

const myTasksUrl = computed(() => (tenantSlug.value ? `/${tenantSlug.value}/my-tasks` : '#'));

const labelsUrl = computed(() => {
  const { tenant, projectKey } = pageContext.routeParams;
  if (typeof tenant === 'string' && typeof projectKey === 'string') {
    return `/${tenant}/projects/${projectKey}/labels`;
  }
  return '#';
});

const routeAlignedTenantId = useRouteAlignedTenantId(
  computed(() => tenantStore.tenants),
  tenantSlug,
);

const projectsQuery = useProjectsQuery(routeAlignedTenantId);

const navProjects = computed(() => projectsQuery.data.value ?? []);

const navProjectsLoading = computed(
  () =>
    projectsQuery.isLoading.value ||
    (Boolean(tenantSlug.value) && routeAlignedTenantId.value === null && tenantStore.isLoading),
);

watch(
  tenantSlug,
  (slug) => {
    if (slug) void tenantStore.loadTenants(slug);
  },
  { immediate: true },
);

function selectTenant(tenant: Tenant) {
  tenantStore.selectTenant(tenant);
  if (tenant.display_id !== tenantSlug.value) {
    // Use a full navigation so tenant-scoped application state is reset.
    window.location.assign(`/${tenant.display_id}/my-tasks`);
  }
}

function retryProjects() {
  void projectsQuery.refetch();
}

// ---- プロジェクト CRUD ダイアログ ----
type ProjectResponse = components['schemas']['ProjectResponse'];

const isCreateProjectOpen = ref(false);
const editingProject = ref<ProjectResponse | null>(null);
const deletingProject = ref<ProjectResponse | null>(null);

function onProjectCreated(project: ProjectResponse) {
  void navigate(`/${tenantSlug.value}/projects/${project.key}/tasks`);
}

function onProjectDeleted(project: ProjectResponse) {
  deletingProject.value = null;
  // 削除したプロジェクトのページを開いていた場合は退避
  if (pageContext.urlPathname.startsWith(`/${tenantSlug.value}/projects/${project.key}/`)) {
    void navigate(`/${tenantSlug.value}/my-tasks`);
  }
}

const data = computed(() => ({
  user: {
    name: authStore.user?.username ?? 'User',
    email: authStore.user?.email ?? '',
    avatar: '/avatars/shadcn.jpg',
  },
  navMain: [
    {
      title: 'My Tasks',
      url: myTasksUrl.value,
      icon: ListTodo,
      isActive: pageContext.urlPathname === `/${pageContext.routeParams.tenant}/my-tasks`,
    },
    {
      title: 'Labels',
      url: labelsUrl.value,
      icon: SquareTerminal,
      isActive: pageContext.urlPathname.endsWith('/labels'),
    },
    {
      title: 'Playground',
      url: '#',
      icon: SquareTerminal,
      items: [
        {
          title: 'History',
          url: '#',
        },
        {
          title: 'Starred',
          url: '#',
        },
        {
          title: 'Settings',
          url: '#',
        },
      ],
    },
    {
      title: 'Models',
      url: '#',
      icon: Bot,
      items: [
        {
          title: 'Genesis',
          url: '#',
        },
        {
          title: 'Explorer',
          url: '#',
        },
        {
          title: 'Quantum',
          url: '#',
        },
      ],
    },
    {
      title: 'Documentation',
      url: '#',
      icon: BookOpen,
      items: [
        {
          title: 'Introduction',
          url: '#',
        },
        {
          title: 'Get Started',
          url: '#',
        },
        {
          title: 'Tutorials',
          url: '#',
        },
        {
          title: 'Changelog',
          url: '#',
        },
      ],
    },
    {
      title: 'Settings',
      url: '#',
      icon: Settings2,
      items: [
        {
          title: 'General',
          url: '#',
        },
        {
          title: 'Team',
          url: '#',
        },
        {
          title: 'Billing',
          url: '#',
        },
        {
          title: 'Limits',
          url: '#',
        },
      ],
    },
  ],
}));
</script>

<template>
  <Sidebar v-bind="props">
    <SidebarHeader>
      <TenantSwitcher
        :tenants="tenantStore.tenants"
        :selected-tenant-id="tenantStore.selectedTenantId"
        :loading="tenantStore.isLoading"
        :error="tenantStore.error"
        @select="selectTenant"
        @retry="tenantStore.loadTenants(tenantSlug)"
      />
    </SidebarHeader>
    <SidebarContent>
      <NavMain :items="data.navMain" />
      <NavProjects
        :tenant-slug="tenantSlug"
        :projects="navProjects"
        :loading="navProjectsLoading"
        :error="projectsQuery.isError.value"
        @retry="retryProjects"
        @create="isCreateProjectOpen = true"
        @edit="(project) => (editingProject = project)"
        @delete="(project) => (deletingProject = project)"
      />
      <template v-if="routeAlignedTenantId">
        <ProjectFormDialog
          :open="isCreateProjectOpen"
          :tenant-id="routeAlignedTenantId"
          @update:open="isCreateProjectOpen = $event"
          @saved="onProjectCreated"
        />
        <ProjectFormDialog
          :open="!!editingProject"
          :tenant-id="routeAlignedTenantId"
          :project="editingProject"
          @update:open="(open) => !open && (editingProject = null)"
        />
        <DeleteProjectDialog
          :open="!!deletingProject"
          :tenant-id="routeAlignedTenantId"
          :project="deletingProject"
          @update:open="(open) => !open && (deletingProject = null)"
          @deleted="onProjectDeleted"
        />
      </template>
    </SidebarContent>
    <SidebarFooter>
      <NavUser :user="data.user" :on-logout="logout" />
    </SidebarFooter>
    <SidebarRail />
  </Sidebar>
</template>
