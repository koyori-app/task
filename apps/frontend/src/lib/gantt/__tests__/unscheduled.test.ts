import { describe, expect, it } from 'vitest';

import { UNASSIGNED_MEMBER } from '@/lib/calendar/filters';

import { projectScope, unscheduledFor } from '../unscheduled';

describe('projectScope', () => {
  it('プロジェクトのガントは、そのプロジェクトで絞る', () => {
    expect(projectScope('p-rel')).toEqual({ project_id: ['p-rel'] });
  });

  it('テナントのガントは絞らない', () => {
    expect(projectScope(null)).toEqual({});
    expect(projectScope(undefined)).toEqual({});
  });
});

describe('unscheduledFor（期間なし N 件の数え方）', () => {
  const tasks = [
    { id: 'a', assignees: [{ user_id: 'u-1' }] },
    { id: 'b', assignees: [{ user_id: 'u-2' }] },
    { id: 'c', assignees: [] },
  ];

  it('担当者で絞らなければ全件を数える', () => {
    expect(unscheduledFor(tasks, [])).toHaveLength(3);
  });

  it('担当者で絞れば、図と同じ絞り方で数える', () => {
    expect(unscheduledFor(tasks, ['u-2']).map((t) => t.id)).toEqual(['b']);
    expect(unscheduledFor(tasks, [UNASSIGNED_MEMBER, 'u-1']).map((t) => t.id)).toEqual(['a', 'c']);
  });
});
