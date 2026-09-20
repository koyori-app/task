import type { GanttDep, GanttTask } from '@koyori-app/arc-vue';
import type { components } from '@/generated/api';

type SprintResponse = components['schemas']['SprintResponse'];
type TaskResponse = components['schemas']['TaskResponse'];
type TaskListResponse = components['schemas']['TaskListResponse'];
type TaskRelationsResponse = components['schemas']['TaskRelationsResponse'];

export interface ArcGanttData {
  tasks: GanttTask[];
  deps: GanttDep[];
}

export interface ArcGanttSources {
  tasks: TaskResponse[];
  sprints: SprintResponse[];
  relations: Map<string, TaskRelationsResponse>;
}

function datePart(value: string): string {
  return value.slice(0, 10);
}

export function toGanttTasks(tasks: TaskResponse[], sprints: SprintResponse[]): GanttTask[] {
  const sprintStarts = new Map(sprints.map((sprint) => [sprint.id, sprint.start_date]));

  return tasks.map((task) => ({
    id: task.id,
    title: task.title,
    progress_pct: task.progress_pct,
    // The product model allows tasks outside a sprint. Their creation date is
    // the least surprising stable fallback until scheduling gets its own field.
    start: task.sprint_id
      ? (sprintStarts.get(task.sprint_id) ?? datePart(task.created_at))
      : datePart(task.created_at),
    // Arc deliberately turns an absent end into start + 1 day.
    end: task.hard_deadline
      ? datePart(task.hard_deadline)
      : task.soft_deadline
        ? datePart(task.soft_deadline)
        : undefined,
  }));
}

export function toGanttDeps(
  tasks: TaskResponse[],
  relations: Map<string, TaskRelationsResponse>,
): GanttDep[] {
  const taskIds = new Set(tasks.map((task) => task.id));
  const seen = new Set<string>();
  const deps: GanttDep[] = [];

  for (const task of tasks) {
    for (const blocked of relations.get(task.id)?.blocks ?? []) {
      if (!taskIds.has(blocked.id)) continue;
      const key = `${task.id}:${blocked.id}`;
      if (seen.has(key)) continue;
      seen.add(key);
      deps.push({ blocker_task_id: task.id, blocked_task_id: blocked.id });
    }
  }

  return deps;
}

async function getJson<T>(url: string, signal?: AbortSignal): Promise<T> {
  const response = await fetch(url, { credentials: 'include', signal });
  if (!response.ok) throw new Error(`${response.status} ${response.statusText}: ${url}`);
  return response.json() as Promise<T>;
}

export async function loadArcGanttData(
  apiBase: string,
  tenantId: string,
  projectId: string,
  signal?: AbortSignal,
): Promise<ArcGanttData> {
  const projectBase =
    `${apiBase}/v1/tenants/${encodeURIComponent(tenantId)}` +
    `/projects/${encodeURIComponent(projectId)}`;
  const sprintsPromise = getJson<SprintResponse[]>(`${projectBase}/sprints`, signal);
  const tasks: TaskResponse[] = [];
  let cursor: string | null = null;

  do {
    const query = new URLSearchParams({ limit: '200' });
    if (cursor) query.set('cursor', cursor);
    const page = await getJson<TaskListResponse>(`${projectBase}/tasks?${query}`, signal);
    tasks.push(...page.tasks);
    cursor = page.next_cursor;
  } while (cursor);

  const relationEntries = await Promise.all(
    tasks.map(
      async (task) =>
        [
          task.id,
          await getJson<TaskRelationsResponse>(
            `${projectBase}/tasks/${encodeURIComponent(task.id)}/relations`,
            signal,
          ),
        ] as const,
    ),
  );
  const sprints = await sprintsPromise;
  const relations = new Map(relationEntries);

  return {
    tasks: toGanttTasks(tasks, sprints),
    deps: toGanttDeps(tasks, relations),
  };
}
