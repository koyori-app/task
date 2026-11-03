import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { defineComponent, nextTick, ref, type Ref } from 'vue';
import { mount } from '@vue/test-utils';
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query';

import { ACTIVITIES_PATH } from '@/composables/useTaskActivities';
import type { ProjectUuid, TenantUuid } from '@/lib/api-ids';
import { TASK_SEARCH_PATH } from '@/lib/api-vue-query';
import { SCHEDULE_PATH } from '@/lib/calendar/schedule';
import { useProjectEvents } from '../useProjectEvents';

class MockEventSource {
  static readonly CLOSED = 2;
  static instances: MockEventSource[] = [];

  readyState = 1;
  close = vi.fn();
  private listeners = new Map<string, Array<() => void>>();

  constructor(
    readonly url: string,
    readonly init: EventSourceInit,
  ) {
    MockEventSource.instances.push(this);
  }

  addEventListener(type: string, listener: () => void) {
    this.listeners.set(type, [...(this.listeners.get(type) ?? []), listener]);
  }

  emit(type: string) {
    for (const listener of this.listeners.get(type) ?? []) listener();
  }
}

const TENANT = 'tenant-1' as TenantUuid;
const PROJECT = 'project-1' as ProjectUuid;

function latestSource() {
  return MockEventSource.instances.at(-1)!;
}

function mountComposable(projectId: Ref<ProjectUuid | null> = ref(PROJECT)) {
  const queryClient = new QueryClient();
  const invalidate = vi.spyOn(queryClient, 'invalidateQueries').mockResolvedValue();
  const wrapper = mount(
    defineComponent({
      setup() {
        useProjectEvents(TENANT, projectId);
        return () => null;
      },
    }),
    { global: { plugins: [[VueQueryPlugin, { queryClient }]] } },
  );
  const invalidatedPaths = () =>
    invalidate.mock.calls.map(([filters]) => (filters as { queryKey: unknown[] }).queryKey[1]);
  return { wrapper, invalidate, invalidatedPaths };
}

describe('useProjectEvents', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    MockEventSource.instances = [];
    vi.stubGlobal('EventSource', MockEventSource);
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });

  it('プロジェクトの events へ Cookie 付きで接続する', () => {
    mountComposable();

    expect(MockEventSource.instances).toHaveLength(1);
    expect(latestSource().url).toBe(`/api/v1/tenants/${TENANT}/projects/${PROJECT}/events`);
    expect(latestSource().init).toEqual({ withCredentials: true });
  });

  it('プロジェクトが未解決の間は接続せず、解決したら接続し、変われば張り直す', async () => {
    const projectId = ref<ProjectUuid | null>(null);
    mountComposable(projectId);
    expect(MockEventSource.instances).toHaveLength(0);

    projectId.value = PROJECT;
    await nextTick();
    expect(MockEventSource.instances).toHaveLength(1);
    const first = latestSource();

    projectId.value = 'project-2' as ProjectUuid;
    await nextTick();
    expect(first.close).toHaveBeenCalledOnce();
    expect(MockEventSource.instances).toHaveLength(2);
    expect(latestSource().url).toContain('/projects/project-2/events');
  });

  it('tasks イベントで対象のキーを無効化する', () => {
    const { invalidatedPaths } = mountComposable();

    latestSource().emit('tasks');
    vi.advanceTimersByTime(300);

    expect(invalidatedPaths()).toEqual(
      expect.arrayContaining([
        '/v1/tenants/{tenant_id}/projects/{project_id}/tasks',
        '/v1/tenants/{tenant_id}/projects/{project_id}/tasks/{id}',
        '/v1/tenants/{tenant_id}/projects/{project_id}/tasks/{id}/relations',
        TASK_SEARCH_PATH,
        SCHEDULE_PATH,
        ACTIVITIES_PATH,
      ]),
    );
  });

  it('300ms 以内の連続イベントは 1 回にまとめ、間を空けた次のイベントは別に扱う', () => {
    const { invalidate } = mountComposable();

    latestSource().emit('tasks');
    vi.advanceTimersByTime(299);
    latestSource().emit('tasks');
    expect(invalidate).not.toHaveBeenCalled();
    vi.advanceTimersByTime(1);
    const callsPerInvalidation = invalidate.mock.calls.length;
    expect(callsPerInvalidation).toBeGreaterThan(0);

    vi.advanceTimersByTime(1000);
    expect(invalidate).toHaveBeenCalledTimes(callsPerInvalidation);

    latestSource().emit('tasks');
    vi.advanceTimersByTime(300);
    expect(invalidate).toHaveBeenCalledTimes(callsPerInvalidation * 2);
  });

  it('open のたびに無効化する（初回も）', () => {
    const { invalidate } = mountComposable();

    latestSource().emit('open');
    vi.advanceTimersByTime(300);
    const callsPerInvalidation = invalidate.mock.calls.length;
    expect(callsPerInvalidation).toBeGreaterThan(0);

    latestSource().emit('error');
    expect(invalidate).toHaveBeenCalledTimes(callsPerInvalidation);

    latestSource().emit('open');
    vi.advanceTimersByTime(300);
    expect(invalidate).toHaveBeenCalledTimes(callsPerInvalidation * 2);
  });

  it('ブラウザが再接続を諦めたら 30 秒後に開き直し、open で無効化する', () => {
    const { invalidate } = mountComposable();
    const first = latestSource();

    first.readyState = MockEventSource.CLOSED;
    first.emit('error');
    vi.advanceTimersByTime(29_999);
    expect(MockEventSource.instances).toHaveLength(1);
    vi.advanceTimersByTime(1);
    expect(MockEventSource.instances).toHaveLength(2);

    latestSource().emit('open');
    vi.advanceTimersByTime(300);
    expect(invalidate).toHaveBeenCalled();
  });

  it('スコープ破棄で close し、保留中の無効化と開き直しも止める', () => {
    const { wrapper, invalidate } = mountComposable();
    const source = latestSource();

    source.emit('tasks');
    source.readyState = MockEventSource.CLOSED;
    source.emit('error');
    wrapper.unmount();
    vi.advanceTimersByTime(60_000);

    expect(source.close).toHaveBeenCalledOnce();
    expect(invalidate).not.toHaveBeenCalled();
    expect(MockEventSource.instances).toHaveLength(1);
  });
});
