/**
 * ストーリーの間だけ、html の data-theme を定める（`beforeEach` に渡す）。
 * 終わったら元の値（無ければ属性ごと）に戻す。
 * Storybook には themeScript を入れないので、属性なし＝暗い配色が既定になる。
 */
export function htmlTheme(theme: 'light' | 'dark') {
  return () => {
    const root = document.documentElement;
    const previous = root.dataset.theme;
    root.dataset.theme = theme;
    return () => {
      if (previous === undefined) delete root.dataset.theme;
      else root.dataset.theme = previous;
    };
  };
}
