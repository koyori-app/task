import { afterEach, describe, expect, it, vi } from 'vitest';

import type { ProjectUuid, TenantUuid } from '@/lib/api-ids';
import { fetchTask } from '@/lib/task-query';

afterEach(() => {
  vi.unstubAllGlobals();
});

const params = {
  path: { tenant_id: 'tenant' as TenantUuid, project_id: 'project' as ProjectUuid, id: 'TASK-1' },
};

describe('fetchTask', () => {
  it('渡された signal で取得を中断できる（詳細と親タスク検索が打ち切れる）', async () => {
    let seen: AbortSignal | undefined;
    vi.stubGlobal('fetch', async (request: Request) => {
      seen = request.signal;
      return Response.json({ title: 't' });
    });
    const controller = new AbortController();
    await fetchTask(params, controller.signal);
    controller.abort();
    expect(seen?.aborted).toBe(true);
  });

  it('404 は null、他の失敗は HTTP status 付きで投げる', async () => {
    vi.stubGlobal('fetch', async () => Response.json({ message: 'nf' }, { status: 404 }));
    await expect(fetchTask(params)).resolves.toBeNull();
    vi.stubGlobal('fetch', async () => Response.json({ message: 'denied' }, { status: 403 }));
    await expect(fetchTask(params)).rejects.toMatchObject({ status: 403, message: 'denied' });
  });
});
