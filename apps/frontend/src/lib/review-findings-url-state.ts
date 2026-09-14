import { SEVERITIES, STATES, type FindingSeverity, type FindingState } from '@/lib/review-findings';

export type ReviewFindingsUrlState = {
  pr: number | null;
  round: number | null;
  severity: FindingSeverity | null;
  state: FindingState | null;
  finding: string | null;
};

export type ParsedReviewFindingsUrlState = {
  state: ReviewFindingsUrlState;
  warnings: string[];
};

export const DEFAULT_REVIEW_FINDINGS_URL_STATE: ReviewFindingsUrlState = {
  pr: null,
  round: null,
  severity: null,
  state: null,
  finding: null,
};

type SearchSource = URLSearchParams | Record<string, string | undefined> | undefined;

function read(search: SearchSource, key: string): string | undefined {
  if (!search) return undefined;
  if (search instanceof URLSearchParams) return search.get(key) ?? undefined;
  return search[key];
}

function displayValue(value: string): string {
  return value.length <= 40 ? value : `${value.slice(0, 37)}…`;
}

function positiveInteger(
  raw: string | undefined,
  label: string,
  warnings: string[],
): number | null {
  if (raw === undefined) return null;
  const value = Number(raw);
  if (Number.isSafeInteger(value) && value > 0) return value;
  warnings.push(`URL の ${label}「${displayValue(raw)}」は正の整数ではないため無視しました。`);
  return null;
}

function knownValue<T extends string>(
  raw: string | undefined,
  label: string,
  values: readonly T[],
  warnings: string[],
): T | null {
  if (raw === undefined) return null;
  if ((values as readonly string[]).includes(raw)) return raw as T;
  warnings.push(`URL の ${label}「${displayValue(raw)}」は知らない値のため無視しました。`);
  return null;
}

function findingId(raw: string | undefined, warnings: string[]): string | null {
  if (raw === undefined) return null;
  if (raw.length > 0 && raw.length <= 128 && /^[A-Za-z0-9_-]+$/.test(raw)) return raw;
  warnings.push(`URL の指摘 ID「${displayValue(raw)}」は不正なため無視しました。`);
  return null;
}

/** SSR と browser の双方で同じ規則を使い、表示状態を URL から復元する。 */
export function parseReviewFindingsUrlState(search: SearchSource): ParsedReviewFindingsUrlState {
  const warnings: string[] = [];
  return {
    state: {
      pr: positiveInteger(read(search, 'pr'), 'PR 番号', warnings),
      round: positiveInteger(read(search, 'round'), 'Round', warnings),
      severity: knownValue(read(search, 'severity'), '重大度', SEVERITIES, warnings),
      state: knownValue(read(search, 'state'), '状態', STATES, warnings),
      finding: findingId(read(search, 'finding'), warnings),
    },
    warnings,
  };
}

/** この画面が所有する query だけを書き換え、既定値は URL へ載せない。 */
export function applyReviewFindingsUrlState(url: URL, state: ReviewFindingsUrlState): URL {
  const next = new URL(url.href);
  for (const key of ['pr', 'round', 'severity', 'state', 'finding']) {
    next.searchParams.delete(key);
  }
  if (state.pr !== null) next.searchParams.set('pr', String(state.pr));
  if (state.round !== null) next.searchParams.set('round', String(state.round));
  if (state.severity !== null) next.searchParams.set('severity', state.severity);
  if (state.state !== null) next.searchParams.set('state', state.state);
  if (state.finding !== null) next.searchParams.set('finding', state.finding);
  return next;
}

export function reviewFindingHref(url: URL, state: ReviewFindingsUrlState, id: string): string {
  return applyReviewFindingsUrlState(url, { ...state, finding: id }).href;
}
