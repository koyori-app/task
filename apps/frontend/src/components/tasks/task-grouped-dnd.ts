/** 押しても行を掴まない場所。入力欄は文字の選択に、行の中に開く一覧は項目の選択に使う。 */
const NOT_DRAG_SOURCE = 'input, textarea, select, [contenteditable="true"], [popover]';

export function canStartRowDrag(target: EventTarget | null): boolean {
  return !(target instanceof Element && target.closest(NOT_DRAG_SOURCE));
}

/** その要素が属する群の status id。群の外なら null。見出しも空の群も余白も群の内。 */
export function statusIdAt(el: EventTarget | null): string | null {
  if (!(el instanceof Element)) return null;
  return el.closest<HTMLElement>('[data-dnd-status-id]')?.dataset['dndStatusId'] ?? null;
}
