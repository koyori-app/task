import {
  tokenizeTaskQuery,
  serializeTaskQuery,
  type TaskQueryKey,
  type QueryToken,
} from './tokenize';
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
export function toggleFlag(expr: string, key: TaskQueryKey, value: string, on: boolean): string {
  if (key === 'no' || key === 'is') {
    const tokens = tokenizeTaskQuery(expr).filter(
      (t) =>
        !(
          t.kind === 'qualifier' &&
          t.key === key &&
          !t.negated &&
          t.values.some((v) => v.toLowerCase() === value.toLowerCase())
        ),
    );
    if (on)
      tokens.push({ kind: 'qualifier', key, values: [value], negated: false, start: 0, end: 0 });
    return serializeTaskQuery(tokens);
  }
  const old = qualifierValues(expr, key);
  return setQualifierValues(
    expr,
    key,
    on ? [...new Set([...old, value])] : old.filter((v) => v !== value),
  );
}
export function removeTokenAt(expr: string, index: number): string {
  return serializeTaskQuery(tokenizeTaskQuery(expr).filter((_, i) => i !== index));
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
