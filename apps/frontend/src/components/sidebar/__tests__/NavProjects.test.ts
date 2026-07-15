import { describe, it, expect, afterEach } from 'vitest';
import { mount, enableAutoUnmount } from '@vue/test-utils';
import type { components } from '@/generated/api';

type ProjectNavItem = components['schemas']['ProjectResponse'];
import NavProjects from '../NavProjects.vue';
import SidebarProvider from '../../ui/sidebar/SidebarProvider.vue';

enableAutoUnmount(afterEach);

const tenantSlug = 'acme';

const sampleProjects: ProjectNavItem[] = [
  {
    id: '00000000-0000-4000-8000-000000000010',
    tenant_id: '00000000-0000-4000-8000-000000000001',
    name: 'Team Alpha',
    description: 'Shared project',
    key: 'alpha',
    is_personal: false,
    icon_emoji: null,
    icon_url: null,
    personal_owner_id: null,
  },
  {
    id: '00000000-0000-4000-8000-000000000020',
    tenant_id: '00000000-0000-4000-8000-000000000001',
    name: 'Personal',
    description: 'Personal project',
    key: 'personal',
    is_personal: true,
    icon_emoji: '📝',
    icon_url: null,
    personal_owner_id: '00000000-0000-4000-8000-000000000099',
  },
];

function mountNavProjects(props: {
  projects?: ProjectNavItem[];
  loading?: boolean;
  error?: boolean;
  tenantSlug?: string;
}) {
  return mount(
    {
      components: { SidebarProvider, NavProjects },
      template: `
        <SidebarProvider>
          <NavProjects
            :tenant-slug="tenantSlug"
            :projects="projects"
            :loading="loading"
            :error="error"
            @retry="onRetry"
          />
        </SidebarProvider>
      `,
      data() {
        return {
          tenantSlug: props.tenantSlug ?? tenantSlug,
          projects: props.projects ?? sampleProjects,
          loading: props.loading ?? false,
          error: props.error ?? false,
          retried: false,
        };
      },
      methods: {
        onRetry(this: { retried: boolean }) {
          this.retried = true;
        },
      },
    },
    { attachTo: document.body },
  );
}

describe('NavProjects', () => {
  it('renders project names and task links', () => {
    const wrapper = mountNavProjects({});
    const links = wrapper.findAll('a');
    const hrefs = links.map((link) => link.attributes('href'));
    expect(hrefs).toContain('/acme/projects/alpha/tasks');
    expect(hrefs).toContain('/acme/projects/personal/tasks');
    expect(wrapper.text()).toContain('Team Alpha');
    expect(wrapper.text()).toContain('Personal');
  });

  it('lists personal projects before shared projects', () => {
    const wrapper = mountNavProjects({});
    const text = wrapper.text();
    expect(text.indexOf('Personal')).toBeLessThan(text.indexOf('Team Alpha'));
  });

  it('shows loading state', () => {
    const wrapper = mountNavProjects({ projects: [], loading: true });
    expect(wrapper.text()).toContain('プロジェクトを読み込み中');
  });

  it('shows empty state', () => {
    const wrapper = mountNavProjects({ projects: [] });
    expect(wrapper.text()).toContain('プロジェクトがありません');
  });

  it('shows error state and emits retry', async () => {
    const wrapper = mountNavProjects({ projects: [], error: true });
    expect(wrapper.text()).toContain('プロジェクト一覧を取得できませんでした');
    await wrapper.find('button').trigger('click');
    expect(wrapper.vm.retried).toBe(true);
  });
});
