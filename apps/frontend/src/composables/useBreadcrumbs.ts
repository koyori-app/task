import { computed, onScopeDispose, ref, toValue, type MaybeRefOrGetter } from 'vue';
import { useQueryClient, type QueryKey } from '@tanstack/vue-query';
import type { components } from '@/generated/api';
import { useTenantStore } from '@/stores/tenant';
import { LIST_PROJECTS_PATH } from '@/lib/api-vue-query';
import { GET_TASK_PATH } from '@/composables/useTaskDetail';
import { TASK_RELATIONS_PATH } from '@/composables/useTaskSubtasks';
import { breadcrumbRoute, breadcrumbSegments, type BreadcrumbState } from '@/lib/breadcrumbs';
import { taskDetailHref, taskListHref } from '@/lib/task-display';

type Tenant = components['schemas']['TenantListItemResponse'];
type Project = components['schemas']['ProjectResponse'];
type Task = components['schemas']['TaskDetailResponse'];
type Relations = components['schemas']['TaskRelationsResponse'];

/** 既存取得の購読だけを行う。ヘッダーのための query observer / 通信は作らない。 */
export function useBreadcrumbs(
  route: MaybeRefOrGetter<{
    urlPathname: string;
    routeParams?: Record<string, string | undefined>;
    is404?: boolean | null;
  }>,
) {
  const client = useQueryClient();
  const tenantStore = useTenantStore();
  const revision = ref(0);
  const unsubscribe = client.getQueryCache().subscribe((event) => {
    if (event.type === 'added' || event.type === 'updated' || event.type === 'removed') {
      revision.value++;
    }
  });
  onScopeDispose(unsubscribe);

  function cached<T>(key: QueryKey) {
    return client.getQueryState<T>(key);
  }

  return computed<BreadcrumbState>(() => {
    // QueryCache は Vue の reactive オブジェクトではないため通知で読み直す。
    void revision.value;
    const page = toValue(route);
    const params = page.routeParams ?? {};
    const descriptor = breadcrumbRoute(page.urlPathname, params);
    const empty = { segments: [], loading: false };
    if (!descriptor || page.is404) return empty;
    const current = { name: descriptor.name, href: page.urlPathname };
    if (descriptor.kind === 'static') {
      return { segments: breadcrumbSegments([current]), loading: false };
    }

    const tenantSlug = encodeURIComponent(params.tenant ?? '');
    const home = { name: 'ホーム', href: `/${tenantSlug}/my-tasks` };
    if (descriptor.kind === 'home') {
      return { segments: breadcrumbSegments([home]), loading: false };
    }
    const tenants = cached<Tenant[]>(['get', '/v1/tenants']);
    if (tenants?.status === 'error') return empty;
    const tenant = (tenants?.data ?? tenantStore.tenants).find(
      (item) => item.display_id === params.tenant,
    );
    if (!tenant) {
      return { ...empty, loading: tenants?.status === 'pending' || tenantStore.isLoading };
    }
    if (descriptor.kind === 'tenant') {
      // Guest には tenant-wide 設定・作成の権限がない。
      if (tenant.membership === 'Guest') return empty;
      return { segments: breadcrumbSegments([home, current]), loading: false };
    }

    const projects = cached<Project[]>([
      'get',
      LIST_PROJECTS_PATH,
      { params: { path: { tenant_id: tenant.id } } },
    ]);
    if (projects?.status === 'error') return empty;
    const project = projects?.data?.find((item) => item.key === params.projectKey);
    if (!project) return { ...empty, loading: !projects || projects.status === 'pending' };
    const projectKey = encodeURIComponent(project.key);
    const projectSegment = { name: project.name, href: taskListHref(tenantSlug, projectKey) };
    if (descriptor.kind === 'project') {
      return {
        segments: breadcrumbSegments([home, projectSegment, ...(current.name ? [current] : [])]),
        loading: false,
      };
    }

    const path = { tenant_id: tenant.id, project_id: project.id, id: params.taskId ?? '' };
    const task = cached<Task | null>(['get', GET_TASK_PATH, { params: { path } }]);
    if (task?.status === 'error' || (task?.status === 'success' && !task.data)) return empty;
    if (!task?.data) return { ...empty, loading: true };
    const relations = cached<Relations>(['get', TASK_RELATIONS_PATH, { params: { path } }]);
    if (task.data.parent_task_id && (!relations || relations.status === 'pending')) {
      return { ...empty, loading: true };
    }
    const parent = relations?.status === 'success' ? relations.data?.parent : null;
    // 深い祖先は辿らず直近の親だけ。古い relations を別の親として描かない。
    const parentSegments =
      parent && parent.id === task.data.parent_task_id
        ? [{ name: parent.title, href: taskDetailHref(tenantSlug, projectKey, parent.seq_id) }]
        : [];
    return {
      segments: breadcrumbSegments([
        home,
        projectSegment,
        ...parentSegments,
        { name: task.data.title, href: taskDetailHref(tenantSlug, projectKey, task.data.seq_id) },
      ]),
      loading: false,
    };
  });
}
