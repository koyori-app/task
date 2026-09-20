import { afterEach, describe, expect, it, vi } from 'vitest';
import { defineComponent, h, nextTick, reactive } from 'vue';
import { enableAutoUnmount, mount } from '@vue/test-utils';
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query';
import { createPinia } from 'pinia';
import AppBreadcrumbs from '@/components/AppBreadcrumbs.vue';
import { useBreadcrumbs } from '@/composables/useBreadcrumbs';
import { GET_TASK_PATH } from '@/composables/useTaskDetail';
import { TASK_RELATIONS_PATH } from '@/composables/useTaskSubtasks';
import { projectsQueryOptions } from '@/lib/api-vue-query';
import type { TenantUuid } from '@/lib/api-ids';
import { toBreadcrumbList } from '@/lib/breadcrumbs';

enableAutoUnmount(afterEach);
afterEach(() => vi.unstubAllGlobals());

const tenant = { id: 'tenant-1', display_id: 'acme', membership: 'Guest', name: '会社' };
const project = {
  id: 'project-1',
  key: 'ENG',
  name: '開発プロジェクト',
  description: '',
  is_personal: false,
  tenant_id: 'tenant-1',
};
const task = { id: 'task-3', seq_id: 3, title: '子タスク', parent_task_id: null as string | null };
const parent = { id: 'task-2', seq_id: 2, title: '親タスク', parent_task_id: 'task-1' };

function queryKey(path: string, id = 'ENG-3') {
  return ['get', path, { params: { path: { tenant_id: tenant.id, project_id: project.id, id } } }];
}

function setup(options: { parent?: boolean; seedTask?: boolean; seedRelations?: boolean } = {}) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const route = reactive({
    urlPathname: '/acme/projects/ENG/tasks/ENG-3',
    routeParams: { tenant: 'acme', projectKey: 'ENG', taskId: 'ENG-3' },
    is404: false,
  });
  client.setQueryData(['get', '/v1/tenants'], [tenant]);
  client.setQueryData(projectsQueryOptions(tenant.id as TenantUuid).queryKey, [project]);
  if (options.seedTask !== false) {
    client.setQueryData(queryKey(GET_TASK_PATH), {
      ...task,
      parent_task_id: options.parent ? parent.id : null,
    });
  }
  if (options.seedRelations !== false) {
    client.setQueryData(queryKey(TASK_RELATIONS_PATH), { parent: options.parent ? parent : null });
  }
  let breadcrumbs!: ReturnType<typeof useBreadcrumbs>;
  const wrapper = mount(
    defineComponent({
      setup() {
        breadcrumbs = useBreadcrumbs(route);
        return () => h(AppBreadcrumbs, breadcrumbs.value);
      },
    }),
    { global: { plugins: [createPinia(), [VueQueryPlugin, { queryClient: client }]] } },
  );
  return {
    client,
    route,
    wrapper,
    get state() {
      return breadcrumbs.value;
    },
  };
}

describe('共通パンくず', () => {
  it.each([false, true])('実データと画面と BreadcrumbList の名を揃える（親=%s）', (hasParent) => {
    const fetch = vi.fn();
    vi.stubGlobal('fetch', fetch);
    const view = setup({ parent: hasParent });
    const names = ['ホーム', project.name, ...(hasParent ? [parent.title] : []), task.title];
    expect(view.state.segments.map((segment) => segment.name)).toEqual(names);
    expect(
      view.wrapper.findAll('[data-slot="breadcrumb-item"]').map((item) => item.text()),
    ).toEqual(names);
    expect(view.wrapper.get('nav').attributes('aria-label')).toBe('パンくず');
    expect(view.wrapper.find('nav > ol').exists()).toBe(true);
    expect(view.wrapper.get('[aria-current="page"]').element.tagName).toBe('SPAN');
    expect(view.wrapper.get('[aria-current="page"]').text()).toBe(task.title);
    expect(view.wrapper.find('a[aria-current]').exists()).toBe(false);
    const links = view.wrapper.findAll('a').map((link) => link.attributes('href'));
    expect(links).toEqual([
      '/acme/my-tasks',
      '/acme/projects/ENG/tasks',
      ...(hasParent ? ['/acme/projects/ENG/tasks/ENG-2'] : []),
    ]);
    const ld = toBreadcrumbList(view.state.segments, 'https://task.example');
    expect(ld.itemListElement.map((item) => item.name)).toEqual(names);
    expect(ld.itemListElement.map((item) => item.position)).toEqual(
      names.map((_, index) => index + 1),
    );
    expect(ld.itemListElement.at(-1)).not.toHaveProperty('item');
    expect(ld.itemListElement.slice(0, -1).map((item) => item.item)).toEqual(
      links.map((link) => `https://task.example${link}`),
    );
    expect(view.wrapper.find('script').exists()).toBe(false);
    expect(fetch).not.toHaveBeenCalled();
  });

  it('二段以上深い親は直近の親までに留め、祖父を追加取得しない', () => {
    const fetch = vi.fn();
    vi.stubGlobal('fetch', fetch);
    const view = setup({ parent: true });
    expect(view.state.segments.map((segment) => segment.name)).toEqual([
      'ホーム',
      project.name,
      parent.title,
      task.title,
    ]);
    expect(fetch).not.toHaveBeenCalled();
  });

  it('未登録ページとエラーページで他ページの段を残さない', async () => {
    const view = setup();
    expect(view.wrapper.find('nav').exists()).toBe(true);
    view.route.urlPathname = '/acme/forgotten-page';
    await nextTick();
    expect(view.state).toEqual({ segments: [], loading: false });
    expect(view.wrapper.find('nav').exists()).toBe(false);
    view.route.urlPathname = '/acme/projects/ENG/tasks/ENG-3';
    view.route.is404 = true;
    await nextTick();
    expect(view.wrapper.find('nav').exists()).toBe(false);
  });

  it('タスクを切り替えた時は骨組みを出し、古い名前を見せない', async () => {
    const view = setup();
    view.route.urlPathname = '/acme/projects/ENG/tasks/ENG-4';
    view.route.routeParams.taskId = 'ENG-4';
    await nextTick();
    expect(view.wrapper.get('[role="status"]').attributes('aria-label')).toBe(
      'パンくずを読み込み中',
    );
    expect(view.wrapper.text()).not.toContain(task.title);
    expect(view.wrapper.get('[data-app-breadcrumbs]').classes()).toContain('h-6');
    view.client.setQueryData(queryKey(GET_TASK_PATH, 'ENG-4'), {
      ...task,
      id: 'task-4',
      seq_id: 4,
      title: '次の仕事',
    });
    await nextTick();
    expect(view.wrapper.get('[aria-current="page"]').text()).toBe('次の仕事');
    expect(view.wrapper.get('[data-app-breadcrumbs]').classes()).toContain('h-6');
  });

  it('親を待つ間は骨組み、失敗時は親の段だけを落とす', async () => {
    const view = setup({ parent: true, seedRelations: false });
    expect(view.state.loading).toBe(true);
    await view.client
      .fetchQuery({
        queryKey: queryKey(TASK_RELATIONS_PATH),
        queryFn: () => Promise.reject(new Error('403')),
      })
      .catch(() => {});
    await nextTick();
    expect(view.state.loading).toBe(false);
    expect(view.state.segments.map((segment) => segment.name)).toEqual([
      'ホーム',
      project.name,
      task.title,
    ]);
  });

  it('タスク取得失敗や404でキャッシュの名前を残さない', async () => {
    const view = setup();
    await view.client
      .fetchQuery({
        queryKey: queryKey(GET_TASK_PATH),
        queryFn: () => Promise.reject(new Error('403')),
      })
      .catch(() => {});
    await nextTick();
    expect(view.state).toEqual({ segments: [], loading: false });
    view.client.setQueryData(queryKey(GET_TASK_PATH), null);
    await nextTick();
    expect(view.state.segments).toEqual([]);
  });

  it('プロジェクト名が取得できなければタスク画面を巻き込まず段を落とす', async () => {
    const view = setup();
    await view.client
      .fetchQuery({
        queryKey: projectsQueryOptions(tenant.id as TenantUuid).queryKey,
        queryFn: () => Promise.reject(new Error('403')),
      })
      .catch(() => {});
    await nextTick();
    expect(view.state).toEqual({ segments: [], loading: false });
  });

  it('テナント変更後は前のテナントの名前やリンクを使わない', async () => {
    const view = setup();
    view.route.routeParams.tenant = 'other';
    view.route.urlPathname = '/other/projects/ENG/tasks/ENG-3';
    await nextTick();
    expect(view.state.segments).toEqual([]);
  });

  it('更新・削除されたキャッシュと同期する', async () => {
    const view = setup();
    view.client.setQueryData(queryKey(GET_TASK_PATH), { ...task, title: '保存した名前' });
    await nextTick();
    expect(view.wrapper.get('[aria-current="page"]').text()).toBe('保存した名前');
    view.client.removeQueries({ queryKey: queryKey(GET_TASK_PATH), exact: true });
    await nextTick();
    expect(view.state.loading).toBe(true);
    expect(view.wrapper.text()).not.toContain('保存した名前');
  });

  it('Guest のリンクは My Tasks・所属プロジェクト・同じプロジェクトの親だけ', async () => {
    const view = setup({ parent: true });
    expect(view.wrapper.findAll('a').map((item) => item.attributes('href'))).toEqual([
      '/acme/my-tasks',
      '/acme/projects/ENG/tasks',
      '/acme/projects/ENG/tasks/ENG-2',
    ]);
    view.route.urlPathname = '/acme/settings';
    await nextTick();
    expect(view.state.segments).toEqual([]);
  });

  it('長い名前は title で全文を読め、狭い画面では一列を横スクロールできる', async () => {
    const view = setup();
    const title = '長い名前</script>\n"'.repeat(40);
    view.client.setQueryData(queryKey(GET_TASK_PATH), { ...task, title });
    await nextTick();
    expect(view.wrapper.get('[aria-current="page"]').attributes('title')).toBe(title);
    expect(view.wrapper.get('[aria-current="page"]').classes()).toContain('truncate');
    expect(view.wrapper.get('ol').classes()).toEqual(
      expect.arrayContaining(['overflow-x-auto', 'flex-nowrap']),
    );
    expect(view.wrapper.find('script').exists()).toBe(false);
    expect(
      toBreadcrumbList(view.state.segments, 'https://task.example').itemListElement.at(-1)?.name,
    ).toBe(title);
  });

  it('アンマウント時に購読を解く', () => {
    const view = setup();
    const listeners = view.client.getQueryCache().hasListeners();
    expect(listeners).toBe(true);
    view.wrapper.unmount();
    expect(view.client.getQueryCache().hasListeners()).toBe(false);
  });
});
