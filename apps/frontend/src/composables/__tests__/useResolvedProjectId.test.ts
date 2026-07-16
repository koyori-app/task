import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { computed, defineComponent } from 'vue';
import { mount, flushPromises } from '@vue/test-utils';
import { VueQueryPlugin, QueryClient, useQuery } from '@tanstack/vue-query';
import type { TenantUuid } from '@/lib/api-ids';

const TENANT_UUID = 'tenant-uuid' as TenantUuid;

const projects = [
  {
    id: 'project-uuid',
    key: 'ENG',
    name: 'Engineering',
    description: '',
    tenant_id: 'tenant-uuid',
    is_personal: false,
  },
];

const { getMock } = vi.hoisted(() => ({
  getMock: vi.fn(async () => ({ data: projects, error: undefined })),
}));

vi.mock('@/lib/api-vue-query', () => ({
  useProjectsQuery: (tenantId: { value: string | null }) =>
    useQuery({
      queryKey: computed(() => ['get', 'projects', tenantId.value]),
      queryFn: async () => {
        const { data, error } = await getMock();
        if (error) throw error;
        return data;
      },
      enabled: computed(() => !!tenantId.value),
    }),
}));

import { useResolvedProjectId } from '../useResolvedProjectId';

function mountComposable(tenantId: TenantUuid | null, projectKey: string) {
  let result!: ReturnType<typeof useResolvedProjectId>;
  const Comp = defineComponent({
    setup() {
      result = useResolvedProjectId(
        computed(() => tenantId),
        computed(() => projectKey),
      );
      return () => null;
    },
  });
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  mount(Comp, { global: { plugins: [[VueQueryPlugin, { queryClient }]] } });
  return { get: () => result, flush: () => flushPromises() };
}

describe('useResolvedProjectId', () => {
  beforeEach(() => {
    getMock.mockClear();
    getMock.mockResolvedValue({ data: projects, error: undefined });
  });
  afterEach(() => vi.clearAllMocks());

  it('project key から project UUID を解決する', async () => {
    const { get, flush } = mountComposable(TENANT_UUID, 'ENG');
    await flush();
    expect(get().projectId.value).toBe('project-uuid');
    expect(get().isProjectNotFound.value).toBe(false);
  });

  it('未知の project key は not-found と判定する', async () => {
    const { get, flush } = mountComposable(TENANT_UUID, 'UNKNOWN');
    await flush();
    expect(get().projectId.value).toBeNull();
    expect(get().isProjectNotFound.value).toBe(true);
  });

  it('tenant UUID が未解決ならリクエストしない', async () => {
    const { get, flush } = mountComposable(null, 'ENG');
    await flush();
    expect(get().projectId.value).toBeNull();
    expect(getMock).not.toHaveBeenCalled();
  });

  it('プロジェクト一覧取得失敗時は isError を立てる', async () => {
    getMock.mockImplementation(async () => {
      throw new Error('server error');
    });
    const { get, flush } = mountComposable(TENANT_UUID, 'ENG');
    await flush();
    expect(get().isProjectNotFound.value).toBe(false);
    expect(get().isError.value).toBe(true);
  });
});
