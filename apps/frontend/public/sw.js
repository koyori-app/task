// task の service worker（scope `/`）。
//
// 何も cache しない。fetch の受け手を置かないので、API・認証の絡む頁・静的な物の
// どの request も、service worker が無い時と同じく network へ行く。
// offline は扱わない（TASK-300 で決める）。
//
// 更新: 新しい版は install でそのまま skipWaiting し、activate で開いておる頁を
// clients.claim する。cache を持たぬので古い画面が残る道は元から無いが、
// 古い service worker が待ちの列に留まらぬよう、すぐ入れ替える。
// この file は消さないこと。/sw.js が無くなると更新の確かめが失敗し、ブラウザは
// 今の service worker を持ち続ける。やめる時は、自らを unregister する版をここに置く。
//
// 通知（TASK-301）: push と notificationclick の受け手は、この file に足す。
// 通知の許可は、利用者の明示の操作（設定の画面のボタンなど）の中でだけ求める。

self.addEventListener('install', () => {
  self.skipWaiting();
});

self.addEventListener('activate', (event) => {
  event.waitUntil(self.clients.claim());
});
