import {
  tokenizeTaskQuery,
  serializeTaskQuery,
  type TaskQueryKey,
  type QueryToken,
} from './tokenize';
import { normalizeQueryValue } from './suggest';
import type { TaskListSortingState } from '@/components/tasks/task-list-sort';
export function qualifierValues(expr: string, key: TaskQueryKey, negated = false): string[] {
  const t = tokenizeTaskQuery(expr).find(
    (t) => t.kind === 'qualifier' && t.key === key && t.negated === negated,
  );
  return t?.kind === 'qualifier' ? t.values : [];
}
export function setQualifierValues(
  expr: string,
  key: TaskQueryKey,
  values: string[],
  negated = false,
): string {
  const tokens = tokenizeTaskQuery(expr);
  const at = tokens.findIndex(
    (t) => t.kind === 'qualifier' && t.key === key && t.negated === negated,
  );
  const t: QueryToken = { kind: 'qualifier', key, values, negated, start: 0, end: 0 };
  if (at < 0) {
    if (values.length) tokens.push(t);
  } else if (values.length) tokens[at] = t;
  else tokens.splice(at, 1);
  return serializeTaskQuery(tokens);
}
/**
 * 除外でない同じキーの項すべてから、値を大小・`@` をそろえて外す。
 * 値が空になった項は消す。同じキーの項が複数あっても、最初の項だけでなく持っている項を直す。
 */
export function removeQualifierValue(expr: string, key: TaskQueryKey, value: string): string {
  const target = normalizeQueryValue(key, value);
  return serializeTaskQuery(
    tokenizeTaskQuery(expr).flatMap((t): QueryToken[] => {
      if (t.kind !== 'qualifier' || t.key !== key || t.negated) return [t];
      const values = t.values.filter((v) => normalizeQueryValue(key, v) !== target);
      return values.length ? [{ ...t, values }] : [];
    }),
  );
}
export function toggleFlag(expr: string, key: TaskQueryKey, value: string, on: boolean): string {
  const removed = removeQualifierValue(expr, key, value);
  if (!on) return removed;
  if (key === 'no' || key === 'is') {
    const tokens = tokenizeTaskQuery(removed);
    tokens.push({ kind: 'qualifier', key, values: [value], negated: false, start: 0, end: 0 });
    return serializeTaskQuery(tokens);
  }
  return setQualifierValues(removed, key, [...qualifierValues(removed, key), value]);
}
export function hasFilter(expr: string): boolean {
  return tokenizeTaskQuery(expr).some((t) => t.kind !== 'qualifier' || t.key !== 'sort');
}
const fields = new Set([
  'title',
  'assignee',
  'priority',
  'due',
  'key',
  'status',
  'created',
  'updated',
]);
export function sortOf(expr: string): TaskListSortingState {
  const value = qualifierValues(expr, 'sort')[0]?.toLowerCase();
  if (!value) return [];
  const match = /^(.+?)(?:-(asc|desc))?$/.exec(value);
  if (!match || !fields.has(match[1]!)) return [];
  return [
    {
      id: match[1] === 'due' ? 'due_date' : match[1]!,
      desc: match[2] ? match[2] === 'desc' : ['created', 'updated'].includes(match[1]!),
    },
  ];
}
export function setSort(expr: string, sorting: TaskListSortingState): string {
  const first = sorting[0];
  const clean = serializeTaskQuery(
    tokenizeTaskQuery(expr).filter((t) => t.kind !== 'qualifier' || t.key !== 'sort'),
  );
  return first
    ? setQualifierValues(clean, 'sort', [
        `${first.id === 'due_date' ? 'due' : first.id}${first.desc ? '-desc' : ''}`,
      ])
    : clean;
}
