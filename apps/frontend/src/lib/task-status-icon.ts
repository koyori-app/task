/** ステータスのアイコンの「円の埋まり方」。並び順から計算するので、ステータスが何個でも出せる。 */
export interface StatusFill {
  /** empty = 点線の円、partial = 扇形、done = 塗りの円にチェック、unknown = 並びに無いステータス */
  kind: 'empty' | 'partial' | 'done' | 'unknown';
  /** 扇形の埋まる割合（0〜1）。partial のときだけ意味を持つ */
  fraction: number;
}

interface OrderedStatus {
  id: string;
  position: number;
  is_done_state: boolean;
}

/**
 * 完了扱いのステータスは塗りの円。未完了は position の昇順に並べ、先頭を点線の円、
 * i 番目（0 始まり）を「i / 未完了の数」だけ埋めた扇形にする。
 */
export function statusFill(
  statuses: readonly OrderedStatus[],
  statusId: string | null | undefined,
): StatusFill {
  const status = statuses.find((s) => s.id === statusId);
  if (!status) return { kind: 'unknown', fraction: 0 };
  if (status.is_done_state) return { kind: 'done', fraction: 1 };
  const open = statuses
    .map((s, index) => ({ s, index }))
    .filter(({ s }) => !s.is_done_state)
    // position が同じなら元の並びを保つ
    .sort((a, b) => a.s.position - b.s.position || a.index - b.index);
  const index = open.findIndex(({ s }) => s.id === status.id);
  if (index <= 0) return { kind: 'empty', fraction: 0 };
  return { kind: 'partial', fraction: index / open.length };
}

interface MenuStatus extends OrderedStatus {
  name: string;
}

/** `@koyori-app/ui-vue` の DropdownItem のうち、ステータスのメニューが使う項目 */
export interface StatusMenuItem {
  value: string;
  label: string;
  kind: 'radio';
  checked: boolean;
  disabled: boolean;
}

/**
 * ステータスのメニューの項目。詳細のボタンと一覧の行のアイコンの両方がここから作る。
 *
 * 今の Dropdown は項目に名前と ✓ しか出せない。項目のアイコン（円の埋まり方）は
 * koyori-app/ui の対応の後にここで足す（UI-45）。
 */
export function statusMenuItems(
  statuses: readonly MenuStatus[],
  statusId: string | null | undefined,
  options: { disabled?: boolean; doneBlocked?: boolean } = {},
): StatusMenuItem[] {
  return statuses.map((status) => ({
    value: status.id,
    label: status.name,
    kind: 'radio',
    checked: status.id === statusId,
    // 完了条件を満たしていない間は、完了扱いのステータスへ直接は移せない（backend が 409 で止める）
    disabled: !!options.disabled || (!!options.doneBlocked && status.is_done_state),
  }));
}
