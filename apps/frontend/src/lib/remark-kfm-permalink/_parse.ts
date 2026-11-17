/**
 * _parse.ts — GitHub のパーマリンクの解釈と、抜粋をコードフェンスに包む処理。
 *
 * 依存を持たない。remark プラグイン (index.ts・SSR 側) と custom element (element.ts・
 * client 側) の両方が import し、「どの URL を抜粋にするか」の判定を 1 か所にする
 * (サーバが抜粋にすると決めた URL を、client が別の規則で読み直さない)。
 * backend の `service::github::snippets::parse_request` も同じ規則で検査する。
 */

/** パーマリンクが指すもの。`path` は復号済みで、区切りは `/`。 */
export type GithubPermalink = {
  readonly owner: string;
  readonly repo: string;
  /** 小文字の 40 桁 */
  readonly sha: string;
  readonly path: string;
  /** 1 始まり。両端を含む */
  readonly start: number;
  readonly end: number;
};

const OWNER_RE = /^[A-Za-z0-9][A-Za-z0-9-]{0,38}$/;
const REPO_RE = /^[A-Za-z0-9._-]{1,100}$/;
const SHA_RE = /^[0-9a-fA-F]{40}$/;
/**
 * URL 全体の形。`URL` で解かずに文字列のまま読む: `URL` は `a/../b` や `%2e%2e`、`\` を
 * 先に畳んでしまい、畳む前の区分を検査できない。scheme と host は完全一致 (ポート・
 * 認証情報つきはここで外れる)。query は無しか `plain=1` だけ。列 (`C5`) は受けるが使わない。
 * 行は 7 桁まで。
 */
const PERMALINK_RE =
  /^https:\/\/github\.com\/([^?#\s\\]+)(?:\?plain=1)?#L(\d{1,7})(?:C\d+)?(?:-L(\d{1,7})(?:C\d+)?)?$/;
// eslint-disable-next-line no-control-regex -- 制御文字を含むパスを弾くための字種
const CONTROL_RE = /[\u0000-\u001f\u007f-\u009f]/;
const MAX_PATH_LENGTH = 1024;

/**
 * `https://github.com/{owner}/{repo}/blob/{sha}/{path}[?plain=1]#L{start}[-L{end}]` だけを読む。
 * ブランチ名・短縮 SHA・行の指定が無い URL は null (GitHub もコミット SHA のリンクだけを展開する)。
 */
export function parseGithubPermalink(href: string): GithubPermalink | null {
  const match = PERMALINK_RE.exec(href);
  if (match === null) return null;
  const start = Number(match[2]);
  const end = match[3] === undefined ? start : Number(match[3]);
  if (start < 1 || end < start) return null;

  // 区分に割る: owner / repo / "blob" / sha / path...
  const [owner, repo, kind, sha, ...encodedPath] = (match[1] ?? '').split('/');
  if (owner === undefined || repo === undefined || sha === undefined) return null;
  if (kind !== 'blob' || encodedPath.length === 0) return null;
  if (!OWNER_RE.test(owner) || !REPO_RE.test(repo) || repo === '.' || repo === '..') return null;
  if (!SHA_RE.test(sha)) return null;

  const segments: string[] = [];
  for (const encoded of encodedPath) {
    let segment: string;
    try {
      segment = decodeURIComponent(encoded);
    } catch {
      return null;
    }
    // 復号した後に見る (`%2e%2e` や `%2F` で区分の検査をすり抜けさせない)
    if (segment === '' || segment === '.' || segment === '..') return null;
    if (segment.includes('/') || CONTROL_RE.test(segment)) return null;
    segments.push(segment);
  }
  const path = segments.join('/');
  if (path.length > MAX_PATH_LENGTH) return null;

  return { owner, repo, sha: sha.toLowerCase(), path, start, end };
}

/** フェンスの情報文字列に置ける字種 (remark-gfm の language クラスと同じ) */
const FLAG_RE = /^[A-Za-z0-9+#._-]+$/;

/**
 * パスから着色の言語指定を決める。拡張子 (無ければファイル名) を小文字にして返す。
 * starry-night は拡張子とファイル名のどちらも言語として解く。知らない名前は着色されないだけ。
 */
export function languageFlagForPath(path: string): string | undefined {
  const basename = path.slice(path.lastIndexOf('/') + 1);
  const dot = basename.lastIndexOf('.');
  const flag = (dot === -1 ? basename : basename.slice(dot + 1)).toLowerCase();
  return FLAG_RE.test(flag) ? flag : undefined;
}

/**
 * 行の列をコードフェンスに包む。フェンスは本文中の最長のバッククォート連続より 1 本長く
 * (最低 3 本) するので、本文がフェンスを閉じることはない。
 */
export function buildSnippetFence(lines: readonly string[], flag?: string): string {
  let longest = 0;
  for (const line of lines) {
    for (const run of line.match(/`+/g) ?? []) {
      longest = Math.max(longest, run.length);
    }
  }
  const fence = '`'.repeat(Math.max(3, longest + 1));
  return `${fence}${flag ?? ''}\n${lines.join('\n')}\n${fence}`;
}
