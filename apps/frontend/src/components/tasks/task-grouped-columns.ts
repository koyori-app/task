import type { components } from '@/generated/api';

type StatusResponse = components['schemas']['ProjectStatusResponse'];
type TaskResponse = components['schemas']['TaskResponse'];

/** ステータス 1 つ分の塊。件数はサーバの total（取得済み件数ではない）。 */
export type TaskGroup = {
  status: StatusResponse;
  tasks: TaskResponse[];
  total: number;
  isLoading: boolean;
  isError: boolean;
  /** まだ取れていない件があるか */
  hasMore: boolean;
  /** List 表示用に API の新しい順を反転し、古い順で並べているか */
  oldestFirst: boolean;
  /** 取得に失敗したページを取り直す。失敗したままだと先へ進めないので導線を出す */
  retry: () => void;
  /** 次のページを足す。カーソルはグループ自身が持つので、呼ぶ側は鍵を知らなくてよい */
  loadMore: () => void;
};

/**
 * List 表示の列定義。
 *
 * ヘッダー（TaskGroupedList）と行（TaskGroupedRow）は別コンポーネントだが、
 * 同じグリッドに載っていないと列がずれる。片方だけ直す事故を防ぐためここに置く。
 *
 * ステータス列は持たない。グループがステータスそのものなので列にすると重複し、
 * 変更はタスク名の左の丸から行う（参照デザイン）。
 * 担当者は複数付くので、アバター 3 つ＋「+N」が収まる幅を取る。
 *
 * 列を並べるのは一覧（`@container`）の幅が 48rem（`@3xl`）以上のときだけ。
 * 狭いときは列を持たず、行の中で 2 段に積む（TaskGroupedRow）。画面幅で分けると、
 * サイドバーを開いた横持ちのように、画面は広いのに一覧が狭い場合を拾えない。
 */
export const TASK_ROW_GRID =
  'grid grid-cols-1 items-start @3xl:grid-cols-[minmax(0,1fr)_7rem_7rem_6rem_4rem]';

/** 列の見出し。列を並べないときは出さない（並べ替えはツールバーのメニューへ移る） */
export const TASK_HEADER_GRID =
  'hidden items-center @3xl:grid @3xl:grid-cols-[minmax(0,1fr)_7rem_7rem_6rem_4rem]';
