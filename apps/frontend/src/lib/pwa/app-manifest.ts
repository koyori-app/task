// web アプリとしての素性（Web App Manifest）の置き場はここ一つ。
// server は /manifest.webmanifest としてこれを返し、+Head.vue は theme-color と
// apple-touch-icon をここから引く。
//
// icon は TASK-299 で新しい絵に差し替えた。絵は仮の物で、清書したら
// public/pwa/ と public/ の画像を同じ名で置き換えるだけで済む。元の絵と焼き方は
// apps/frontend/design/app-icon/ にある。色は仮の値のまま変えていない
// （icon の地は background_color と同じ白で塗っている）。

/** header（bg-sidebar の --sidebar: oklch(0.985 0 0)）に合わせた仮の色。 */
export const PWA_THEME_COLOR = '#fafafa';
/** 起動の間に出る地の色。--background: oklch(1 0 0) に合わせた仮の色。icon の地もこの白。 */
export const PWA_BACKGROUND_COLOR = '#ffffff';

/** 元の絵（design/app-icon/source.png）を焼いた icon（TASK-299）。どれも不透明で、透けは白で埋めてある。 */
export const PWA_ICONS = {
  icon192: '/pwa/icon-192.png',
  icon512: '/pwa/icon-512.png',
  maskable512: '/pwa/icon-maskable-512.png',
  appleTouch: '/pwa/apple-touch-icon.png',
} as const;

/**
 * id・start_url・scope は後で変えない。
 *
 * - id はブラウザが install 済みのアプリを見分ける鍵である。変えると、install 済みの
 *   者には別のアプリとして扱われる。origin の根 `/` に置けば、URL の構成（テナントの
 *   slug など）が変わっても揺らがない。
 * - start_url は `/`。テナントへの振り分けは `/` の頁が既にしておるので、ここに
 *   テナントや利用者を埋めない（埋めると、別の者や別のテナントで開いた時に食い違う）。
 * - scope は `/`。アプリの全ての頁を standalone の窓の内に収め、service worker の
 *   scope（/sw.js の `/`）と揃える。
 */
export const webAppManifest = {
  id: '/',
  name: 'task',
  short_name: 'task',
  start_url: '/',
  scope: '/',
  display: 'standalone',
  lang: 'ja',
  theme_color: PWA_THEME_COLOR,
  background_color: PWA_BACKGROUND_COLOR,
  icons: [
    { src: PWA_ICONS.icon192, sizes: '192x192', type: 'image/png' },
    { src: PWA_ICONS.icon512, sizes: '512x512', type: 'image/png' },
    { src: PWA_ICONS.maskable512, sizes: '512x512', type: 'image/png', purpose: 'maskable' },
  ],
} as const;

/** タブの favicon。icon と同じ元の絵を縮めた物で、透けたまま出す（タブの地の色に乗せるため）。 */
export const FAVICONS = [
  { href: '/favicon-16.png', sizes: '16x16' },
  { href: '/favicon-32.png', sizes: '32x32' },
] as const;

export const MANIFEST_PATH = '/manifest.webmanifest';
export const MANIFEST_CONTENT_TYPE = 'application/manifest+json; charset=utf-8';
