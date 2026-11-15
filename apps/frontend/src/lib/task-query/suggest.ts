import { TASK_LIST_SORT_COLUMNS } from '@/components/tasks/task-list-sort';
import { TASK_QUERY_KEYS, tokenizeTaskQuery, quoteValue, type TaskQueryKey } from './tokenize';
export type SuggestContext =
  | { kind: 'keys'; prefix: string; negated: boolean; range: [number, number] }
  | {
      kind: 'values';
      key: TaskQueryKey;
      prefix: string;
      chosen: string[];
      negated: boolean;
      range: [number, number];
    };
export type Candidate = {
  value: string;
  label: string;
  count?: number;
  hint?: string;
  key?: TaskQueryKey;
};
export type SuggestData = {
  statuses?: { id: string; name: string }[];
  labels?: { id: string; name: string }[];
  members?: { id: string; username: string }[];
  facets?: {
    statuses: { id: string; count: number }[];
    labels: { id: string; count: number }[];
    assignees: { id: string; count: number }[];
    authors: { id: string; count: number }[];
    priorities: { priority: string; count: number }[];
    unassigned: number;
    unlabeled: number;
  };
};
export const KEY_HELP: Record<TaskQueryKey, [string, string]> = {
  status: ['ステータス', 'status:"In Progress"'],
  assignee: ['担当者', 'assignee:@me'],
  label: ['ラベル', 'label:bug,feature'],
  priority: ['優先度', 'priority:high'],
  due: ['期限', 'due:<7d'],
  no: ['未設定', 'no:assignee'],
  is: ['タスクの種類', 'is:subtask'],
  in: ['探す場所', 'in:comment'],
  author: ['作成者', 'author:@me'],
  sort: ['並び順', 'sort:priority'],
};
export function suggestionContext(draft: string, caret: number): SuggestContext | null {
  const tokens = tokenizeTaskQuery(draft);
  const t = tokens.find((t) => t.start <= caret && t.end >= caret);
  const from = t?.start ?? caret;
  const raw = draft.slice(from, caret);
  const negated = raw.startsWith('-');
  const text = negated ? raw.slice(1) : raw;
  const colon = text.indexOf(':');
  if (colon < 0) {
    if (!text || TASK_QUERY_KEYS.some((k) => k.startsWith(text.toLowerCase())))
      return { kind: 'keys', prefix: text, negated, range: [from, t?.end ?? caret] };
    return null;
  }
  const key = text.slice(0, colon).toLowerCase() as TaskQueryKey;
  if (!TASK_QUERY_KEYS.includes(key)) return null;
  const partial = tokenizeTaskQuery(raw)[0];
  const values = partial?.kind === 'qualifier' ? partial.values : [];
  return {
    kind: 'values',
    key,
    prefix: values.at(-1) ?? '',
    chosen: values.slice(0, -1),
    negated,
    range: [from, t?.end ?? caret],
  };
}
export function keyCandidates(prefix: string): Candidate[] {
  return TASK_QUERY_KEYS.filter((k) => k.startsWith(prefix.toLowerCase())).map((k) => ({
    value: k,
    label: `${k}:`,
    hint: `${KEY_HELP[k][0]}　${KEY_HELP[k][1]}`,
  }));
}
export const PRIORITY_QUERY_LABELS: Record<string, string> = {
  critical_fire: '緊急',
  critical: '重大',
  high: '高',
  medium: '中',
  low: '低',
  trivial: '些細',
};
const priorityApiNames: Record<string, string> = {
  critical_fire: 'CriticalFire',
  critical: 'Critical',
  high: 'High',
  medium: 'Medium',
  low: 'Low',
  trivial: 'Trivial',
};
const fixed: Partial<Record<TaskQueryKey, string[]>> = {
  priority: ['critical_fire', 'critical', 'high', 'medium', 'low', 'trivial'],
  due: ['overdue', '<1d', '<7d', '<30d'],
  no: ['assignee', 'label'],
  is: ['archived', 'subtask'],
  in: ['title', 'comment'],
  sort: ['created', 'updated', 'key', 'status', 'title', 'assignee', 'priority', 'due'],
};
export function valueCandidates(key: TaskQueryKey, prefix: string, data: SuggestData): Candidate[] {
  let choices: Candidate[] = [];
  const facet =
    key === 'status'
      ? data.facets?.statuses
      : key === 'label'
        ? data.facets?.labels
        : key === 'assignee'
          ? data.facets?.assignees
          : data.facets?.authors;
  if (key === 'status' || key === 'label')
    choices =
      (key === 'status' ? data.statuses : data.labels)?.map((v) => ({
        value: v.name,
        label: v.name,
        count: facet?.find((f) => f.id === v.id)?.count,
      })) ?? [];
  else if (key === 'assignee' || key === 'author')
    choices = [
      { value: '@me', label: '自分（@me）' },
      ...(data.members ?? []).map((v) => ({
        value: v.username,
        label: v.username,
        count: facet?.find((f) => f.id === v.id)?.count,
      })),
    ];
  else
    choices = (fixed[key] ?? []).map((value) => ({
      value,
      label:
        key === 'priority'
          ? (PRIORITY_QUERY_LABELS[value] ?? value)
          : key === 'sort'
            ? (TASK_LIST_SORT_COLUMNS.find((c) => c.id === (value === 'due' ? 'due_date' : value))
                ?.label ?? value)
            : value,
      hint: value,
      count:
        key === 'priority'
          ? data.facets?.priorities.find((p) => p.priority === priorityApiNames[value])?.count
          : undefined,
    }));
  const normalize = (v: string) =>
    (key === 'assignee' || key === 'author' ? v.replace(/^@/, '') : v).toLowerCase();
  const p = normalize(prefix);
  choices = choices
    .filter((c) => normalize(c.value).includes(p))
    .sort(
      (a, b) =>
        Number(normalize(a.value) !== p) - Number(normalize(b.value) !== p) ||
        Number(!normalize(a.value).startsWith(p)) - Number(!normalize(b.value).startsWith(p)),
    );
  if (key === 'label' || key === 'assignee')
    choices.push({
      value: key,
      label: key === 'label' ? 'ラベルなし' : '担当者なし',
      key: 'no',
      count: key === 'label' ? data.facets?.unlabeled : data.facets?.unassigned,
    });
  return choices;
}
export function applySuggestion(
  draft: string,
  context: SuggestContext,
  candidate: Candidate,
): { draft: string; commit: boolean } {
  const text =
    context.kind === 'keys'
      ? `${context.negated ? '-' : ''}${candidate.value}:`
      : `${context.negated ? '-' : ''}${candidate.key ?? context.key}:${[...(candidate.key ? [] : context.chosen), candidate.value].map(quoteValue).join(',')}`;
  return {
    draft: draft.slice(0, context.range[0]) + text + draft.slice(context.range[1]),
    commit: context.kind === 'values',
  };
}
