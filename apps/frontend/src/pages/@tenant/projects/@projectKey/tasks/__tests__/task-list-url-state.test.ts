import { describe, it, expect } from 'vitest';
import { effectScope, ref, nextTick } from 'vue';
import {
  parseTaskListUrlState,
  applyTaskListUrlState,
  clampPage,
  useTaskListUrlSync,
  useLegacyLabelMigration,
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
  it('retains legacy label and page until migration completes', () => {
    const url = new URL('http://localhost/tasks?label=x&page=3');
    const pending = applyTaskListUrlState(url, { q: '', page: 3, view: 'list' }, true);
    expect(pending.searchParams.get('label')).toBe('x');
    expect(pending.searchParams.get('page')).toBe('3');
    expect(
      applyTaskListUrlState(pending, { q: 'label:bug', page: 3, view: 'list' }).searchParams.has(
        'label',
      ),
    ).toBe(false);
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

describe('legacy label migration', () => {
  const labels = [{ id: 'x', name: 'bug' }];
  function setup(initial: typeof labels | null, isError = false, query = 'sort:title') {
    const data = ref<typeof labels | null>(initial);
    const error = ref(isError);
    const q = ref(query);
    const pending = useLegacyLabelMigration({
      legacyLabelId: 'x',
      labels: data,
      isError: error,
      query: q,
    });
    return { data, error, q, pending };
  }
  it('converts the label id once the labels arrive', async () => {
    const t = setup(null);
    expect(t.pending.value).toBe(true);
    t.data.value = labels;
    await nextTick();
    expect(t.q.value).toBe('sort:title label:bug');
    expect(t.pending.value).toBe(false);
  });
  it('gives up and releases the wait when fetching labels fails', async () => {
    const t = setup(null);
    t.error.value = true;
    await nextTick();
    expect(t.pending.value).toBe(false);
    expect(t.q.value).toBe('sort:title');
    // 後から届いても変換しない
    t.data.value = labels;
    await nextTick();
    expect(t.q.value).toBe('sort:title');
  });
  it('drops the label from the URL and clamps the page after a failure', async () => {
    window.history.replaceState(null, '', '/tasks?label=x&page=3');
    const t = setup(null);
    const pagination = ref({ pageIndex: 2, pageSize: 20 });
    const scope = effectScope();
    scope.run(() =>
      useTaskListUrlSync({
        legacyLabelPending: t.pending,
        selectedTaskId: ref(null),
        view: ref('table'),
        pagination,
        query: t.q,
        taskTotal: ref(10),
        isPagerActive: ref(true),
      }),
    );
    expect(new URL(window.location.href).searchParams.get('label')).toBe('x');
    expect(pagination.value.pageIndex).toBe(2);
    t.error.value = true;
    await nextTick();
    expect(new URL(window.location.href).searchParams.has('label')).toBe(false);
    expect(pagination.value.pageIndex).toBe(0);
    scope.stop();
  });
  it('leaves the wait in place while labels are still loading', async () => {
    const t = setup(null);
    await nextTick();
    expect(t.pending.value).toBe(true);
  });
});
