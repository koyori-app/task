/**
 * タスク作成ダイアログのプロパティ列に並べるチップの見た目。
 *
 * Tailwind が拾えるよう、class は完成した文字列で書く（組み立てない）。
 */

/** 自前の button / label に付けるチップの見た目（高さ 28px、角丸の枠） */
export const CHIP_CLASS =
  'inline-flex h-7 shrink-0 items-center gap-1.5 rounded-full border border-(--koyori-color-border) px-3 text-[0.8125rem] leading-5 text-(--koyori-color-text) hover:bg-(--koyori-color-surface-hover) focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-(--koyori-focus-color)';
/** 未設定（項目名を薄く出す） */
export const CHIP_EMPTY_CLASS = 'text-(--koyori-color-text-muted)';
/** 開いている間 */
export const CHIP_OPEN_CLASS =
  'bg-(--koyori-color-surface-hover) border-(--koyori-color-border-strong)';

/**
 * koyori の Picker / Dropdown / UserSelector の根に付け、中の引き金（根の直下の button）を
 * チップにする。字の色は設定済み / 未設定で替えるので、ここには含めない。
 *
 * `--button-border` は koyori の基底 `._button`（0,1,0）で宣言されるので、`.x>button`（0,1,1）で勝つ。
 * 字の色は ghost の `--button-color`（0,2,0）に負けるため、`color` を直接上書きする。
 */
const CHIP_TRIGGER_BASE =
  'shrink-0 [&>button]:rounded-full [&>button]:font-normal [&>button]:[--koyori-control-height-sm:28px] [&>button]:[--button-border:var(--koyori-color-border)] [&>button[aria-expanded=true]]:[--button-background:var(--koyori-color-surface-hover)]';
const CHIP_TRIGGER_SET = '[&>button]:text-(--koyori-color-text)';
const CHIP_TRIGGER_EMPTY = '[&>button]:text-(--koyori-color-text-muted)';

/** koyori の引き金をチップにする class。値が無い（未設定の）ときは字を薄くする */
export function chipTrigger(empty: boolean): string {
  return `${CHIP_TRIGGER_BASE} ${empty ? CHIP_TRIGGER_EMPTY : CHIP_TRIGGER_SET}`;
}
