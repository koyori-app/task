<script setup lang="ts">
import type { SidebarProps } from '@/components/ui/sidebar';
import { useAuthSession } from '@/composables/useAuthSession';
import { useAuthStore } from '@/stores/auth';
import { useTenantStore, type Tenant } from '@/stores/tenant';
import { usePageContext } from 'vike-vue/usePageContext';
import { computed, onMounted } from 'vue';

import {
  BookOpen,
  Bot,
  Frame,
  ListTodo,
  Map,
  PieChart,
  Settings2,
  SquareTerminal,
} from '@lucide/vue';
import NavMain from '@/components/sidebar/NavMain.vue';
import NavProjects from '@/components/sidebar/NavProjects.vue';
import NavUser from '@/components/sidebar/NavUser.vue';
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

onMounted(() => tenantStore.loadTenants(tenantSlug.value));

function selectTenant(tenant: Tenant) {
  tenantStore.selectTenant(tenant);
  if (tenant.display_id !== tenantSlug.value) {
    // Use a full navigation so tenant-scoped application state is reset.
    window.location.assign(`/${tenant.display_id}/my-tasks`);
  }
}

// This is sample data.
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
  projects: [
    {
      name: 'Design Engineering',
      url: '#',
      icon: Frame,
    },
    {
      name: 'Sales & Marketing',
      url: '#',
      icon: PieChart,
    },
    {
      name: 'Travel',
      url: '#',
      icon: Map,
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
      <NavProjects :projects="data.projects" />
    </SidebarContent>
    <SidebarFooter>
      <NavUser :user="data.user" :on-logout="logout" />
    </SidebarFooter>
    <SidebarRail />
  </Sidebar>
</template>
