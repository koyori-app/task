/**
 * サイドバーからページへ移ったときに、狭い画面のサイドバー（モーダルの Drawer）を閉じるかどうか。
 *
 * 狭い画面のサイドバーはページに重なって出るので、開いたままだと遷移先が隠れ、
 * 移動したことが一見して分からない。個々のリンクに閉じる処理を付けると追加の
 * たびに漏れるため、ナビが集まる要素でまとめて拾う。
 *
 * 広い画面では Drawer がモーダルでなく、閉じる指示を受けても何も変わらないので、
 * ここでは画面幅を見ない。
 *
 * 委譲の下に入らないリンク（`DropdownMenuPortal` でサイドバーの外へ出る
 * ユーザーメニューなど）は、同じ判定をそのリンク側で呼ぶ。
 */
export function shouldCloseSidebarOnNavigate(event: MouseEvent): boolean {
  // 修飾キー付き・左クリック以外は別タブで開くだけで、このページは動かない
  if (event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) {
    return false;
  }
  return !!(event.target as Element | null)?.closest?.('a');
}
