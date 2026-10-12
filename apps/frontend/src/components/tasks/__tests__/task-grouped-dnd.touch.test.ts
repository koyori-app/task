/**
 * タッチ端末での DnD の開始条件の試験。
 *
 * lib の既定では指が動いた瞬間に drag が始まり、touchmove を preventDefault して
 * スクロールを打ち消す。行がリストのほぼ全面を覆うので、スマホで一覧をスクロール
 * できなくなる。gesture は jsdom 系では再現できぬので、dragAndDrop をすり替えて
 * config を受け取る（task-grouped-dnd.rejection.test.ts と同じ）。
 */

import { afterEach, describe, expect, it, vi } from 'vitest';
import { enableAutoUnmount, mount } from '@vue/test-utils';

import TaskGroupedList from '@/components/tasks/TaskGroupedList.vue';
import type { TaskGroup } from '@/components/tasks/task-grouped-columns';
import type { components } from '@/generated/api';

type StatusResponse = components['schemas']['ProjectStatusResponse'];
type TaskResponse = components['schemas']['TaskResponse'];

const configs = vi.hoisted(() => [] as Array<Record<string, unknown>>);

vi.mock('@formkit/drag-and-drop/vue', () => ({
  dragAndDrop: (config: Record<string, unknown>) => {
    configs.push(config);
  },
}));

enableAutoUnmount(afterEach);
afterEach(() => {
  configs.length = 0;
});

const todo: StatusResponse = {
  id: 'status-todo',
  name: 'Todo',
  color: '#94a3b8',
  position: 0,
  is_default: true,
  is_done_state: false,
  is_default_done: false,
  project_id: 'project-1',
  created_at: '2026-06-01T00:00:00Z',
};

const row: TaskResponse = {
  id: 'task-1',
  project_id: 'project-1',
  seq_id: 1,
  title: 'task 1',
  description: null,
  status_id: todo.id,
  priority: 'Medium',
  progress_pct: 0,
  soft_deadline: null,
  hard_deadline: null,
  is_archived: false,
  assignees: [],
  labels: [],
  created_at: '2026-06-01T00:00:00Z',
  updated_at: '2026-06-01T00:00:00Z',
};

const todoGroup: TaskGroup = {
  status: todo,
  tasks: [row],
  total: 1,
  isLoading: false,
  isError: false,
  hasMore: false,
  oldestFirst: true,
  retry: () => {},
  loadMore: () => {},
};

describe('TaskGroupedList の DnD（タッチ）', () => {
  it('長押ししてから動かしたときだけ掴む（指を滑らせたらスクロールに任せる）', () => {
    mount(TaskGroupedList, {
      props: {
        groups: [todoGroup],
        tenantId: 'tenant-1',
        projectId: 'project-1',
        statuses: [todo],
        projectLabels: [],
        members: [],
        pending: {},
        errors: {},
        sorting: [],
        onComment: vi.fn(async () => true),
        onCreate: vi.fn(async () => true),
        onMoveStatus: vi.fn(async () => {}),
      },
      attachTo: document.body,
    });

    expect(configs.length).toBeGreaterThan(0);
    for (const config of configs) {
      expect(config['longPress']).toBe(true);
      expect(config['longPressDuration']).toBeGreaterThan(0);
    }
  });
});
