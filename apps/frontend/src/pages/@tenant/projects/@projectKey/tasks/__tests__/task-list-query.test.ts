import { describe, it, expect } from 'vitest';
import { buildTasksListQueryParams, taskListPlaceholderData } from '../task-list-query';
describe('task list conditions', () => {
  it('sends q and timezone with pagination, including subtasks', () => {
    expect(
      buildTasksListQueryParams('t', 'p', { pageIndex: 2, pageSize: 20 }, 'label:bug', 'Asia/Tokyo')
        .params.query,
    ).toEqual({ limit: 20, offset: 40, q: 'label:bug', timezone: 'Asia/Tokyo' });
  });
  it('only reuses data for the same project and expression', () => {
    const data = { tasks: [] };
    const query = {
      queryKey: [
        'get',
        '/tasks',
        buildTasksListQueryParams('t', 'p', { pageIndex: 0, pageSize: 20 }, 'label:bug'),
      ],
    };
    expect(taskListPlaceholderData(data, query, 'p', 'label:bug')).toBe(data);
    expect(taskListPlaceholderData(data, query, 'p', 'label:feature')).toBeUndefined();
    expect(taskListPlaceholderData(data, query, 'other', 'label:bug')).toBeUndefined();
  });
});
