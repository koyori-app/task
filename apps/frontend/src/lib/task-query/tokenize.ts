export const TASK_QUERY_KEYS = [
  'status',
  'assignee',
  'label',
  'priority',
  'due',
  'no',
  'is',
  'in',
  'author',
  'sort',
] as const;
export type TaskQueryKey = (typeof TASK_QUERY_KEYS)[number];
export type QueryToken = (
  | { kind: 'word' | 'phrase'; text: string; negated: boolean }
  | { kind: 'qualifier'; key: TaskQueryKey; values: string[]; negated: boolean }
) & { start: number; end: number };
const space = (c: string | undefined) =>
  c !== undefined &&
  ((c.charCodeAt(0) >= 9 && c.charCodeAt(0) <= 13) || [32, 160, 12288].includes(c.charCodeAt(0)));
export function tokenizeTaskQuery(input: string): QueryToken[] {
  const tokens: QueryToken[] = [];
  let at = 0;
  function phrase() {
    at++;
    let text = '';
    while (at < input.length) {
      const c = input[at++]!;
      if (c === '"') break;
      if (c === '\\' && (input[at] === '"' || input[at] === '\\')) text += input[at++]!;
      else text += c;
    }
    return text;
  }
  while (at < input.length) {
    if (space(input[at])) {
      at++;
      continue;
    }
    const start = at;
    const negated = input[at] === '-' && at + 1 < input.length && !space(input[at + 1]);
    if (negated) at++;
    if (input[at] === '"') {
      const text = phrase();
      tokens.push({ kind: 'phrase', text, negated, start, end: at });
      continue;
    }
    const bodyStart = at;
    let keyEnd = at;
    while (
      keyEnd < input.length &&
      !space(input[keyEnd]) &&
      input[keyEnd] !== ':' &&
      input[keyEnd] !== '"'
    )
      keyEnd++;
    const key = input.slice(at, keyEnd).toLowerCase() as TaskQueryKey;
    if (input[keyEnd] === ':' && TASK_QUERY_KEYS.includes(key)) {
      at = keyEnd + 1;
      const values: string[] = [];
      do {
        if (input[at] === '"') values.push(phrase());
        else {
          const from = at;
          while (at < input.length && !space(input[at]) && input[at] !== ',' && input[at] !== '"')
            at++;
          values.push(input.slice(from, at));
        }
        if (input[at] !== ',') break;
        at++;
      } while (at <= input.length);
      tokens.push({ kind: 'qualifier', key, values, negated, start, end: at });
    } else {
      while (at < input.length && !space(input[at])) at++;
      tokens.push({ kind: 'word', text: input.slice(bodyStart, at), negated, start, end: at });
    }
  }
  return tokens;
}
export function quoteValue(value: string): string {
  return !value || Array.from(value).some((c) => space(c) || c === ',' || c === '"')
    ? `"${value.replaceAll('\\', '\\\\').replaceAll('"', '\\"')}"`
    : value;
}
export function serializeToken(token: QueryToken): string {
  const prefix = token.negated ? '-' : '';
  return (
    prefix +
    (token.kind === 'qualifier'
      ? `${token.key}:${token.values.map(quoteValue).join(',')}`
      : token.kind === 'phrase'
        ? `"${token.text.replaceAll('\\', '\\\\').replaceAll('"', '\\"')}"`
        : token.text)
  );
}
export function serializeTaskQuery(tokens: QueryToken[]): string {
  return [
    ...tokens.filter((t) => t.kind === 'qualifier'),
    ...tokens.filter((t) => t.kind !== 'qualifier'),
  ]
    .map(serializeToken)
    .join(' ');
}
