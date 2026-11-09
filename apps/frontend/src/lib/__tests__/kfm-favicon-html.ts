// 外部リンクの先頭に出る favicon の img (sanitize 通過後の実出力)。試験の期待 HTML 用
export const favicon = (host: string): string =>
  `<img class="kfm-link-favicon" src="/api/v1/favicons/${host}" alt="" aria-hidden="true" loading="lazy">`;
