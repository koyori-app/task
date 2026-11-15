import { describe, it, expect } from 'vitest';
import { effectScope, ref, nextTick } from 'vue';
import {
  parseTaskListUrlState,
  applyTaskListUrlState,
  clampPage,
  useTaskListUrlSync,
} from '../task-list-url-state';
describe('condition expression URL state', () => {
  it('restores expressions and converts the first legacy sort', () => {
    expect(
      parseTaskListUrlState({
        q: ' 通知 ',
        label: 'id',
        sort: 'title:asc,priority:desc',
        view: 'table',
        page: '3',
      }),
    ).toEqual({ q: 'sort:title 通知', legacyLabelId: 'id', view: 'table', page: 3 });
    expect(parseTaskListUrlState({ q: 'sort:priority', sort: 'title:asc' }).q).toBe(
      'sort:priority',
    );
  });
  it.each(['__proto__', 'constructor', 'toString', 'valueOf', 'labels', 'unknown'])(
    'rejects unsafe legacy column %s',
    (id) => expect(parseTaskListUrlState({ sort: `${id}:asc` }).q).toBe(''),
  );
  it.each(['key', 'status', 'title', 'priority', 'assignee', 'due_date'])(
    'keeps supported legacy sort %s',
    (id) =>
      expect(parseTaskListUrlState({ sort: `${id}:desc` }).q).toBe(
        `sort:${id === 'due_date' ? 'due' : id}-desc`,
      ),
  );
  it('writes q and removes legacy fields, preserving selection', () => {
    const url = applyTaskListUrlState(
      new URL('http://localhost/tasks?label=x&sort=x&selected=ENG-1'),
      { q: 'label:bug', page: 2, view: 'table' },
    );
    expect(url.searchParams.get('q')).toBe('label:bug');
    expect(url.searchParams.has('label')).toBe(false);
    expect(url.searchParams.has('sort')).toBe(false);
    expect(url.searchParams.get('selected')).toBe('ENG-1');
  });
  it.each(['0', '-1', '1.2', 'x'])('rounds invalid page %s', (page) =>
    expect(parseTaskListUrlState({ page }).page).toBe(1),
  );
  it('clamps pages, including zero results', () => {
    expect(clampPage(9, 21, 20)).toBe(2);
    expect(clampPage(9, 0, 20)).toBe(1);
    expect(clampPage(-1, 21, 20)).toBe(1);
  });
  it('only clamps table pagination after total is loaded', async () => {
    const scope = effectScope();
    const pagination = ref({ pageIndex: 8, pageSize: 20 });
    const total = ref<number | null>(null);
    const active = ref(false);
    scope.run(() =>
      useTaskListUrlSync({
        selectedTaskId: ref(null),
        view: ref('list'),
        pagination,
        query: ref(''),
        taskTotal: total,
        isPagerActive: active,
      }),
    );
    total.value = 0;
    await nextTick();
    expect(pagination.value.pageIndex).toBe(8);
    active.value = true;
    await nextTick();
    expect(pagination.value.pageIndex).toBe(0);
    scope.stop();
  });
});
