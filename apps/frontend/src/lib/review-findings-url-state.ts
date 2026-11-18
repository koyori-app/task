import { SEVERITIES, STATES, type FindingSeverity, type FindingState } from '@/lib/review-findings';
import { PR_STATE_FILTERS, type PrStateFilter } from '@/lib/review-pr-rail';

/** PR 一覧の検索語を URL から読むときの上限。超えた分は黙って切る */
export const PR_QUERY_MAX_LENGTH = 200;

export type ReviewFindingsUrlState = {
  pr: number | null;
  round: number | null;
  severity: FindingSeverity | null;
  state: FindingState | null;
  finding: string | null;
  /** PR 一覧を自分が作成した PR だけに絞るか（`mine=1`）。 */
  mine: boolean;
  /** PR 一覧のページ（1 始まり）。null は「表示中の PR が載っているページ」で、URL に載せない。 */
  page: number | null;
  /** PR 一覧の検索語（`q`）。空は URL に載せない。 */
  q: string;
  /**
   * PR 一覧の状態の絞り込み（`pr_state`）。null は「選んでいない」で、一覧が届いてから
   * 既定の決め方（`initialPrStateFilter`）に任せる。選んだ状態は `open` でも載せる。
   */
  prState: PrStateFilter | null;
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
  mine: false,
  page: null,
  q: '',
  prState: null,
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

function mineFlag(raw: string | undefined, warnings: string[]): boolean {
  if (raw === undefined) return false;
  if (raw === '1') return true;
  warnings.push(
    `URL の「自分の PR だけ」の値「${displayValue(raw)}」は知らない値のため無視しました。`,
  );
  return false;
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
      mine: mineFlag(read(search, 'mine'), warnings),
      page: positiveInteger(read(search, 'page'), 'ページ', warnings),
      q: (read(search, 'q') ?? '').slice(0, PR_QUERY_MAX_LENGTH),
      prState: knownValue(read(search, 'pr_state'), 'PR の状態', PR_STATE_FILTERS, warnings),
    },
    warnings,
  };
}

/** この画面が所有する query だけを書き換え、既定値は URL へ載せない。 */
export function applyReviewFindingsUrlState(url: URL, state: ReviewFindingsUrlState): URL {
  const next = new URL(url.href);
  for (const key of [
    'pr',
    'round',
    'severity',
    'state',
    'finding',
    'mine',
    'page',
    'q',
    'pr_state',
  ]) {
    next.searchParams.delete(key);
  }
  if (state.pr !== null) next.searchParams.set('pr', String(state.pr));
  if (state.round !== null) next.searchParams.set('round', String(state.round));
  if (state.severity !== null) next.searchParams.set('severity', state.severity);
  if (state.state !== null) next.searchParams.set('state', state.state);
  if (state.finding !== null) next.searchParams.set('finding', state.finding);
  if (state.mine) next.searchParams.set('mine', '1');
  // 1 ページ目も明示されたなら残す。page が無い URL は「表示中の PR が載るページ」を指すため、
  // 省くと 2 ページ目の PR を選んだまま 1 ページ目に戻した状態を再表示で復元できない
  if (state.page !== null) next.searchParams.set('page', String(state.page));
  // 読む側と同じ上限で切る（書いた URL を開き直しても同じ検索語になるように）
  if (state.q !== '') next.searchParams.set('q', state.q.slice(0, PR_QUERY_MAX_LENGTH));
  // 選んだ状態は `open` でも書く。Open が 1 件も無い一覧の既定は「すべて」なので、
  // 省くと明示した Open が再表示で「すべて」になる
  if (state.prState !== null) next.searchParams.set('pr_state', state.prState);
  return next;
}

export function reviewFindingHref(url: URL, state: ReviewFindingsUrlState, id: string): string {
  return applyReviewFindingsUrlState(url, { ...state, finding: id }).href;
}

/**
 * 指摘への链を pathname + search の相対形で返す。SSR と client が同じ属性文字列を
 * 出すための唯一の口——base の origin に依らず、base が持つこの画面の外の query
 * （tab= の類）も保たれる。両側の一致はこの関数の試験が契約として固める。
 */
export function relativeReviewFindingHref(
  base: URL,
  state: ReviewFindingsUrlState,
  id: string,
): string {
  const next = new URL(reviewFindingHref(base, state, id));
  return `${next.pathname}${next.search}`;
}
