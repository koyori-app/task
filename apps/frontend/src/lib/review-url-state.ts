import { SEVERITIES, STATES, type FindingSeverity, type FindingState } from '@/lib/review-findings';
import {
  PR_REVIEWED_FILTERS,
  PR_SORTS,
  PR_STATE_FILTERS,
  PR_UNRESOLVED_FILTERS,
  type PrReviewedFilter,
  type PrSort,
  type PrStateFilter,
  type PrUnresolvedFilter,
} from '@/lib/review-pr-list';

/** 検索語を URL から読むときの上限。超えた分は黙って切る */
export const PR_QUERY_MAX_LENGTH = 200;

/** PR の一覧（`/reviews`）が URL に持つ状態。 */
export type ReviewListUrlState = {
  /** 検索語（`q`）。空は URL に載せない。 */
  q: string;
  /**
   * PR の状態の絞り込み（`pr_state`）。null は「選んでいない」で、一覧が届いてから
   * 既定の決め方（`initialPrStateFilter`）に任せる。選んだ状態は `open` でも載せる。
   */
  prState: PrStateFilter | null;
  /** 作成者（`author`。GitHub のユーザー名）。 */
  author: string | null;
  unresolved: PrUnresolvedFilter;
  reviewed: PrReviewedFilter;
  /** 自分が作成した PR だけに絞るか（`mine=1`）。 */
  mine: boolean;
  sort: PrSort;
  /** ページ（1 始まり）。null は 1 ページ目で、URL に載せない。 */
  page: number | null;
};

/** PR の詳細（`/reviews/:prNumber`）が URL に持つ状態。 */
export type ReviewDetailUrlState = {
  q: string;
  /** 状態の絞り込み（`state`。カンマ区切り）。空は絞らない。 */
  states: FindingState[];
  /** 重大度の絞り込み（`severity`。カンマ区切り）。空は絞らない。 */
  severities: FindingSeverity[];
  round: number | null;
  /** レビュアー（`reviewer`。利用者の ID）。 */
  reviewer: string | null;
  /** 開いている指摘（`finding`）。 */
  finding: string | null;
};

export const DEFAULT_REVIEW_LIST_URL_STATE: ReviewListUrlState = {
  q: '',
  prState: null,
  author: null,
  unresolved: 'all',
  reviewed: 'all',
  mine: false,
  sort: 'recent',
  page: null,
};

export const DEFAULT_REVIEW_DETAIL_URL_STATE: ReviewDetailUrlState = {
  q: '',
  states: [],
  severities: [],
  round: null,
  reviewer: null,
  finding: null,
};

type SearchSource = URLSearchParams | Record<string, string | undefined> | undefined;

const LIST_KEYS = ['q', 'pr_state', 'author', 'unresolved', 'reviewed', 'mine', 'sort', 'page'];
const DETAIL_KEYS = ['q', 'state', 'severity', 'round', 'reviewer', 'finding'];

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
  if (raw.trim() !== '' && Number.isSafeInteger(value) && value > 0) return value;
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

/** カンマ区切りの集合。知らない値は警告を出して捨て、重複をまとめて `values` の順に並べる。 */
function knownValues<T extends string>(
  raw: string | undefined,
  label: string,
  values: readonly T[],
  warnings: string[],
): T[] {
  if (raw === undefined) return [];
  const found = new Set<string>();
  for (const part of raw.split(',')) {
    if ((values as readonly string[]).includes(part)) found.add(part);
    else warnings.push(`URL の ${label}「${displayValue(part)}」は知らない値のため無視しました。`);
  }
  return values.filter((value) => found.has(value));
}

function identifier(raw: string | undefined, label: string, warnings: string[]): string | null {
  if (raw === undefined) return null;
  if (raw.length > 0 && raw.length <= 128 && /^[A-Za-z0-9_-]+$/.test(raw)) return raw;
  warnings.push(`URL の${label}「${displayValue(raw)}」は不正なため無視しました。`);
  return null;
}

function authorName(raw: string | undefined, warnings: string[]): string | null {
  if (raw === undefined) return null;
  if (raw.length > 0 && raw.length <= 100) return raw;
  warnings.push(`URL の作成者「${displayValue(raw)}」は不正なため無視しました。`);
  return null;
}

function mineFlag(raw: string | undefined, warnings: string[]): boolean {
  if (raw === undefined) return false;
  if (raw === '1') return true;
  warnings.push(
    `URL の「自分の PR だけ」の値「${displayValue(raw)}」は知らない値のため無視しました。`,
  );
  return false;
}

function query(search: SearchSource): string {
  return (read(search, 'q') ?? '').slice(0, PR_QUERY_MAX_LENGTH);
}

/** SSR と browser の双方で同じ規則を使い、一覧の表示状態を URL から復元する。 */
export function parseReviewListUrlState(search: SearchSource): {
  state: ReviewListUrlState;
  warnings: string[];
} {
  const warnings: string[] = [];
  return {
    state: {
      q: query(search),
      prState: knownValue(read(search, 'pr_state'), 'PR の状態', PR_STATE_FILTERS, warnings),
      author: authorName(read(search, 'author'), warnings),
      unresolved:
        knownValue(read(search, 'unresolved'), '未解決', PR_UNRESOLVED_FILTERS, warnings) ?? 'all',
      reviewed:
        knownValue(read(search, 'reviewed'), '最終レビュー', PR_REVIEWED_FILTERS, warnings) ??
        'all',
      mine: mineFlag(read(search, 'mine'), warnings),
      sort: knownValue(read(search, 'sort'), '並び', PR_SORTS, warnings) ?? 'recent',
      page: positiveInteger(read(search, 'page'), 'ページ', warnings),
    },
    warnings,
  };
}

/** 一覧が持つ query だけを書き換え、既定値は URL へ載せない。 */
export function applyReviewListUrlState(url: URL, state: ReviewListUrlState): URL {
  const next = new URL(url.href);
  for (const key of LIST_KEYS) next.searchParams.delete(key);
  // 読む側と同じ上限で切る（書いた URL を開き直しても同じ検索語になるように）
  if (state.q !== '') next.searchParams.set('q', state.q.slice(0, PR_QUERY_MAX_LENGTH));
  // 選んだ状態は `open` でも書く。Open が 1 件も無い一覧の既定は「すべて」なので、
  // 省くと明示した Open が再表示で「すべて」になる
  if (state.prState !== null) next.searchParams.set('pr_state', state.prState);
  if (state.author !== null) next.searchParams.set('author', state.author);
  if (state.unresolved !== 'all') next.searchParams.set('unresolved', state.unresolved);
  if (state.reviewed !== 'all') next.searchParams.set('reviewed', state.reviewed);
  if (state.mine) next.searchParams.set('mine', '1');
  if (state.sort !== 'recent') next.searchParams.set('sort', state.sort);
  if (state.page !== null && state.page > 1) next.searchParams.set('page', String(state.page));
  return next;
}

/** SSR と browser の双方で同じ規則を使い、詳細の表示状態を URL から復元する。 */
export function parseReviewDetailUrlState(search: SearchSource): {
  state: ReviewDetailUrlState;
  warnings: string[];
} {
  const warnings: string[] = [];
  return {
    state: {
      q: query(search),
      states: knownValues(read(search, 'state'), '状態', STATES, warnings),
      severities: knownValues(read(search, 'severity'), '重大度', SEVERITIES, warnings),
      round: positiveInteger(read(search, 'round'), 'Round', warnings),
      reviewer: identifier(read(search, 'reviewer'), 'レビュアー', warnings),
      finding: identifier(read(search, 'finding'), '指摘 ID', warnings),
    },
    warnings,
  };
}

/** 詳細が持つ query だけを書き換え、既定値は URL へ載せない。 */
export function applyReviewDetailUrlState(url: URL, state: ReviewDetailUrlState): URL {
  const next = new URL(url.href);
  for (const key of DETAIL_KEYS) next.searchParams.delete(key);
  if (state.q !== '') next.searchParams.set('q', state.q.slice(0, PR_QUERY_MAX_LENGTH));
  if (state.states.length > 0) {
    next.searchParams.set('state', STATES.filter((s) => state.states.includes(s)).join(','));
  }
  if (state.severities.length > 0) {
    next.searchParams.set(
      'severity',
      SEVERITIES.filter((s) => state.severities.includes(s)).join(','),
    );
  }
  if (state.round !== null) next.searchParams.set('round', String(state.round));
  if (state.reviewer !== null) next.searchParams.set('reviewer', state.reviewer);
  if (state.finding !== null) next.searchParams.set('finding', state.finding);
  return next;
}

/**
 * 指摘へのリンクを pathname + search の相対形で返す。SSR と client が同じ属性文字列を
 * 出すための唯一の口。base の origin に依らず、base が持つこの画面の外の query
 * （tab= の類）も保たれる。両側の一致はこの関数の試験が契約として固める。
 */
export function relativeReviewDetailFindingHref(
  base: URL,
  state: ReviewDetailUrlState,
  id: string,
): string {
  const next = applyReviewDetailUrlState(base, { ...state, finding: id });
  return `${next.pathname}${next.search}`;
}

/** URL のパスの PR 番号。正の整数でなければ null。 */
export function parsePrNumber(raw: string | undefined): number | null {
  if (raw === undefined || !/^[1-9]\d*$/.test(raw)) return null;
  const value = Number(raw);
  return Number.isSafeInteger(value) ? value : null;
}

export function reviewListHref(tenant: string, projectKey: string): string {
  return `/${tenant}/projects/${projectKey}/reviews`;
}

export function reviewDetailHref(tenant: string, projectKey: string, prNumber: number): string {
  return `${reviewListHref(tenant, projectKey)}/${prNumber}`;
}

/**
 * 旧 URL（`/reviews?pr=N`）の転送先。`pr` が無い・不正なら null。
 *
 * 指摘の絞り込みと開いている指摘（`state` `severity` `round` `finding`）は値をそのまま
 * 引き継ぎ（不正な値の警告は転送先が出す）、一覧のクエリ（`mine` `page` `q` `pr_state`）は捨てる。
 */
export function legacyReviewDetailHref(pathname: string, search: SearchSource): string | null {
  const pr = parsePrNumber(read(search, 'pr'));
  if (pr === null) return null;
  const params = new URLSearchParams();
  for (const key of ['state', 'severity', 'round', 'finding']) {
    const value = read(search, key);
    if (value !== undefined) params.set(key, value);
  }
  const query = params.toString();
  return `${pathname.replace(/\/$/, '')}/${pr}${query ? `?${query}` : ''}`;
}
