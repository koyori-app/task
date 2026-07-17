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
            @create="onCreate"
            @edit="onEdit"
            @delete="onDelete"
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
          created: false,
          edited: null as ProjectNavItem | null,
          deleted: null as ProjectNavItem | null,
        };
      },
      methods: {
        onRetry(this: { retried: boolean }) {
          this.retried = true;
        },
        onCreate(this: { created: boolean }) {
          this.created = true;
        },
        onEdit(this: { edited: ProjectNavItem | null }, project: ProjectNavItem) {
          this.edited = project;
        },
        onDelete(this: { deleted: ProjectNavItem | null }, project: ProjectNavItem) {
          this.deleted = project;
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
    const retryButton = wrapper
      .findAll('button')
      .find((b) => b.text().includes('プロジェクト一覧を取得できませんでした'));
    expect(retryButton).toBeTruthy();
    await retryButton!.trigger('click');
    expect(wrapper.vm.retried).toBe(true);
  });

  it('does not render invalid task links when tenant slug is empty', () => {
    const wrapper = mountNavProjects({ tenantSlug: '' });
    const links = wrapper.findAll('a');
    const hrefs = links.map((link) => link.attributes('href'));
    expect(hrefs).not.toContain('#');
    expect(hrefs.some((href) => href?.includes('/projects/'))).toBe(false);
    expect(wrapper.text()).toContain('Team Alpha');
  });

  it('emits create when the group action is clicked', async () => {
    const wrapper = mountNavProjects({});
    const createButton = wrapper.find('button[title="プロジェクトを作成"]');
    expect(createButton.exists()).toBe(true);
    await createButton.trigger('click');
    expect(wrapper.vm.created).toBe(true);
  });

  it('hides the create action when tenant slug is empty', () => {
    const wrapper = mountNavProjects({ tenantSlug: '' });
    expect(wrapper.find('button[title="プロジェクトを作成"]').exists()).toBe(false);
  });

  it('shows the kebab menu only for non-personal projects', () => {
    const wrapper = mountNavProjects({});
    const kebabs = wrapper.findAll('button[aria-label$="の操作"]');
    expect(kebabs.length).toBe(1);
    expect(kebabs[0].attributes('aria-label')).toBe('Team Alpha の操作');
  });

  it('emits edit/delete with the project from the kebab menu', async () => {
    const wrapper = mountNavProjects({});
    const kebab = wrapper.find('button[aria-label="Team Alpha の操作"]');
    await kebab.trigger('click');
    // DropdownMenuContent は body 直下にポータルされる
    const items = [...document.body.querySelectorAll('[role="menuitem"]')];
    const editItem = items.find((el) => el.textContent?.includes('編集'));
    expect(editItem).toBeTruthy();
    (editItem as HTMLElement).click();
    await wrapper.vm.$nextTick();
    expect((wrapper.vm as unknown as { edited: ProjectNavItem | null }).edited?.name).toBe(
      'Team Alpha',
    );

    await kebab.trigger('click');
    const deleteItem = [...document.body.querySelectorAll('[role="menuitem"]')].find((el) =>
      el.textContent?.includes('削除'),
    );
    expect(deleteItem).toBeTruthy();
    (deleteItem as HTMLElement).click();
    await wrapper.vm.$nextTick();
    expect((wrapper.vm as unknown as { deleted: ProjectNavItem | null }).deleted?.name).toBe(
      'Team Alpha',
    );
  });
});
