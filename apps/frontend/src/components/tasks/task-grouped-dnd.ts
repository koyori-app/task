import type { components } from '@/generated/api';

type TaskResponse = components['schemas']['TaskResponse'];

/** onTransfer から要る分だけ。lib の型へ依存を広げぬ。 */
export type DndTransferLike = {
  targetParent: { el: HTMLElement };
  draggedNodes: Array<{ data: { value: TaskResponse } }>;
};

/**
 * 群をまたぐ transfer を update:status へ写す。
 *
 * 移した先の群は container の data-dnd-status-id で判ずる。
 * 同じ status への transfer と、印の無い container への transfer は流さぬ
 * （lib の座標判定が揺れた時に、無駄な PATCH を打たぬため）。
 */
export function emitTransferAsStatusChange(
  data: DndTransferLike,
  emitStatus: (task: TaskResponse, statusId: string) => void,
): void {
  const targetStatusId = data.targetParent.el.dataset['dndStatusId'];
  const moved = data.draggedNodes[0]?.data.value;
  if (moved && targetStatusId && moved.status_id !== targetStatusId) {
    emitStatus(moved, targetStatusId);
  }
}
