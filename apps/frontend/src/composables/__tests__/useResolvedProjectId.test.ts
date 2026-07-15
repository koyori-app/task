import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { computed, defineComponent } from 'vue';
import { mount, flushPromises } from '@vue/test-utils';
import { VueQueryPlugin, QueryClient } from '@tanstack/vue-query';

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

vi.mock('@/lib/api-vue-query', () => ({ fetchClient: { GET: getMock } }));

import { useResolvedProjectId } from '../useResolvedProjectId';

function mountComposable(tenantId: string | null, projectKey: string) {
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
    const { get, flush } = mountComposable('tenant-uuid', 'ENG');
    await flush();
    expect(get().projectId.value).toBe('project-uuid');
    expect(get().isProjectNotFound.value).toBe(false);
  });

  it('未知の project key は not-found と判定する', async () => {
    const { get, flush } = mountComposable('tenant-uuid', 'UNKNOWN');
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
    const { get, flush } = mountComposable('tenant-uuid', 'ENG');
    await flush();
    expect(get().isProjectNotFound.value).toBe(false);
    expect(get().isError.value).toBe(true);
  });
});
