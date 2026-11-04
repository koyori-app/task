/** 開いたボタンの近くに出すパネルの位置。画面の端で切れるなら上下左右を反転する。 */
export interface AnchorRect {
  left: number;
  right: number;
  top: number;
  bottom: number;
}

export function placeAnchored(
  anchor: AnchorRect,
  size: { width: number; height: number },
  viewport: { width: number; height: number },
  { gap = 4, margin = 8 }: { gap?: number; margin?: number } = {},
): { left: number; top: number } {
  const roomBelow = viewport.height - margin - anchor.bottom - gap;
  const roomAbove = anchor.top - gap - margin;
  const below = size.height <= roomBelow || roomBelow >= roomAbove;
  const top = below ? anchor.bottom + gap : anchor.top - gap - size.height;

  const fitsRight = anchor.left + size.width <= viewport.width - margin;
  const left = fitsRight ? anchor.left : anchor.right - size.width;

  return {
    left: clamp(left, margin, viewport.width - margin - size.width),
    top: clamp(top, margin, viewport.height - margin - size.height),
  };
}

/** 画面より大きければ左上（`min`）を優先する */
function clamp(value: number, min: number, max: number): number {
  return Math.max(min, Math.min(value, max));
}
