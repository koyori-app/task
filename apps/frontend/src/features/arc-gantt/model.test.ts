import type { components } from '@/generated/api';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { loadArcGanttData, toGanttDeps, toGanttTasks } from './model';

type SprintResponse = components['schemas']['SprintResponse'];
type TaskResponse = components['schemas']['TaskResponse'];
type TaskRelationsResponse = components['schemas']['TaskRelationsResponse'];

function task(overrides: Partial<TaskResponse> = {}): TaskResponse {
  return {
    id: 'task-1',
    project_id: 'project-1',
    seq_id: 1,
    title: '実データの課題',
    status_id: 'status-1',
    priority: 'Medium',
    progress_pct: 35,
    sprint_id: 'sprint-1',
    is_archived: false,
    assignees: [],
    labels: [],
    created_at: '2026-09-10T12:00:00Z',
    updated_at: '2026-09-20T12:00:00Z',
    ...overrides,
  };
}

function requestUrl(input: string | URL | Request): string {
  if (typeof input === 'string') return input;
  return input instanceof URL ? input.href : input.url;
}

afterEach(() => vi.unstubAllGlobals());

describe('arc Gantt adapter', () => {
  it("uses sprint.start_date and leaves end absent for Arc's +1 day fallback", () => {
    const sprints = [{ id: 'sprint-1', start_date: '2026-09-15' }] as SprintResponse[];
    expect(toGanttTasks([task()], sprints)).toEqual([
      {
        id: 'task-1',
        title: '実データの課題',
        progress_pct: 35,
        start: '2026-09-15',
        end: undefined,
      },
    ]);
  });

  it('falls back to created_at for tasks outside a sprint', () => {
    expect(toGanttTasks([task({ sprint_id: null })], [])).toMatchObject([{ start: '2026-09-10' }]);
  });

  it('maps only dependencies whose two tasks are in the loaded chart', () => {
    const tasks = [task(), task({ id: 'task-2', seq_id: 2 })];
    const relations = new Map<string, TaskRelationsResponse>([
      [
        'task-1',
        {
          parent: null,
          subtasks: [],
          blocked_by: [],
          blocks: [{ ...task({ id: 'task-2' }), relation_id: 'relation-1' }],
        },
      ],
    ]);
    expect(toGanttDeps(tasks, relations)).toEqual([
      { blocker_task_id: 'task-1', blocked_task_id: 'task-2' },
    ]);
  });

  it('loads the existing task, sprint, and relation routes', async () => {
    const fetchMock = vi.fn(async (input: string | URL | Request) => {
      const url = requestUrl(input);
      if (url.endsWith('/sprints')) return Response.json([]);
      if (url.endsWith('/relations')) {
        return Response.json({ parent: null, subtasks: [], blocks: [], blocked_by: [] });
      }
      return Response.json({ tasks: [task({ sprint_id: null })], total: 1, next_cursor: null });
    });
    vi.stubGlobal('fetch', fetchMock);

    await expect(loadArcGanttData('/api', 'tenant id', 'project/id')).resolves.toMatchObject({
      tasks: [{ id: 'task-1', start: '2026-09-10' }],
      deps: [],
    });
    expect(fetchMock.mock.calls.map(([url]) => requestUrl(url))).toEqual(
      expect.arrayContaining([
        '/api/v1/tenants/tenant%20id/projects/project%2Fid/sprints',
        '/api/v1/tenants/tenant%20id/projects/project%2Fid/tasks?limit=200',
        '/api/v1/tenants/tenant%20id/projects/project%2Fid/tasks/task-1/relations',
      ]),
    );
  });
});
