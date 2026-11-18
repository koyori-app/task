/**
 * element.ts — `<kfm-permalink>` custom element (client 専用・軽量シェル)。
 *
 * SSR HTML は `<kfm-permalink><a href="パーマリンク">…</a></kfm-permalink>` (remark-kfm-permalink)。
 * この要素が backend から抜粋を取り、リンクをコードの抜粋に差し替える。サーバがコードを
 * HTML に入れない理由は index.ts のコメントを正とする (描画結果は全利用者で共有される)。
 *
 * - 抜粋: `GET /api/v1/github/snippet`。見せてよい相手かは backend が決める。
 * - 着色: 抜粋をコードフェンスに包んで既存の `POST /internal/render-description` に送り、
 *   返った HTML (サーバで sanitize 済み。v-html と同じ信頼) の `<kfm-code>` を使う。着色を
 *   client に持つと +417.5 KB になる (_client-registry のコメント)。着色に失敗したら、
 *   行を textContent で入れた無着色の `<kfm-code>` を DOM で組む (HTML として解釈させない)。
 * - 取れないとき (未連携・権限なし・ファイルなし・通信の失敗) は中のリンクをそのまま残す。
 *   理由は出さない (連携の有無を画面から推し量らせない。backend も 404 にまとめている)。
 * - 抜粋を覚える場所と長さ (見せてよいかは backend の認可が決めるので、認可を通らずに返る経路を短く保つ):
 *   - ブラウザーの HTTP キャッシュ: 使わない。backend が `no-store` を返し、fetch も `cache: 'no-store'`。
 *     URL が同じでも利用者で結果が変わるので、残すと同じブラウザーの次の利用者に渡る
 *   - この module の Map: 取れた抜粋を取得の開始から 5 分だけ。期限を過ぎた表示は backend の認可を
 *     通り直す (権限を外された後、開いたままのタブが古い抜粋を出し続けない)。失敗は覚えない。
 *     ページの読み直し (ログアウトを含む) で消える
 *   - backend の Redis: 認可の後ろにあるので、取り直しは安い
 * - constructor は factory で遅延 (_client-registry 契約)。静的 import は _tag と _parse だけ。
 *
 * VRT / E2E 向けの完了シグナル (時間待ち不要の口):
 * - 成功: `data-kfm-permalink="rendered"` が立ち、中身がヘッダと `<kfm-code>` になる
 * - 取れない: `data-kfm-permalink="unavailable"` が立ち、リンクが残る
 * - 属性なし: 未処理 (取得中 or JS 無効)
 */
import { buildSnippetFence, languageFlagForPath, parseGithubPermalink } from './_parse';
import type { GithubPermalink } from './_parse';
import { KFM_PERMALINK_TAG } from './_tag';

export { KFM_PERMALINK_TAG } from './_tag';

export type KfmPermalinkState = 'rendered' | 'unavailable';

type LoadedSnippet = {
  readonly startLine: number;
  readonly endLine: number;
  readonly lines: readonly string[];
  readonly truncated: boolean;
  /** 着色済みの HTML。着色できなかったときは null */
  readonly html: string | null;
};

/** 覚えておく抜粋の件数。超えたら全部捨てる (長く開いた画面で膨らませない) */
const SNIPPET_CACHE_MAX = 200;

/**
 * 取れた抜粋を覚えておく時間。これが「権限を外された後も新しい表示に抜粋が出る時間」の上限になる
 * (HTTP キャッシュは使わないので、ほかの期限と足し合わされない)。
 */
export const SNIPPET_CACHE_TTL_MS = 5 * 60 * 1000;

type CacheEntry = {
  readonly pending: Promise<LoadedSnippet>;
  /** 取得を始めた時刻 (壁時計の ms) */
  readonly startedAt: number;
};

/** 同じ抜粋の取得を 1 回にまとめる (同じリンクが本文とコメントに並ぶことがある) */
const snippetCache = new Map<string, CacheEntry>();

function cacheKey(target: GithubPermalink): string {
  const repo = `${target.owner}/${target.repo}`.toLowerCase();
  return `${repo}@${target.sha}:${target.path}#${target.start}-${target.end}`;
}

async function fetchSnippet(target: GithubPermalink): Promise<LoadedSnippet> {
  const query = new URLSearchParams({
    owner: target.owner,
    repo: target.repo,
    sha: target.sha,
    path: target.path,
    start: String(target.start),
    end: String(target.end),
  });
  // HTTP キャッシュを使わせない (応答は利用者で変わる。backend の no-store と二重にする)
  const response = await fetch(`/api/v1/github/snippet?${query.toString()}`, {
    cache: 'no-store',
  });
  if (response.status !== 200) throw new Error(`snippet responded ${response.status}`);
  const body: unknown = await response.json();
  if (typeof body !== 'object' || body === null) throw new Error('snippet body is not an object');
  const {
    start_line: startLine,
    end_line: endLine,
    lines,
    truncated,
  } = body as Record<string, unknown>;
  if (
    typeof startLine !== 'number' ||
    typeof endLine !== 'number' ||
    !Array.isArray(lines) ||
    lines.length === 0 ||
    !lines.every((line): line is string => typeof line === 'string')
  ) {
    throw new Error('snippet body has an unexpected shape');
  }
  return {
    startLine,
    endLine,
    lines,
    truncated: truncated === true,
    html: await highlight(lines, languageFlagForPath(target.path)),
  };
}

/** 抜粋を着色した HTML を返す。着色できないときは null (無着色で出す) */
async function highlight(
  lines: readonly string[],
  flag: string | undefined,
): Promise<string | null> {
  try {
    const response = await fetch('/internal/render-description', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ description: buildSnippetFence(lines, flag), scope: 'permalink' }),
    });
    if (response.status !== 200) return null;
    const body: unknown = await response.json();
    const html =
      typeof body === 'object' && body !== null ? (body as { html?: unknown }).html : null;
    return typeof html === 'string' ? html : null;
  } catch {
    return null;
  }
}

function loadSnippet(target: GithubPermalink): Promise<LoadedSnippet> {
  const key = cacheKey(target);
  // 壁時計で測る。performance.now() は端末のスリープの間に進まない環境があり
  // (macOS / Linux の Chromium)、開いたままのタブで期限がスリープの分だけ延びる。
  // 壁時計が巻き戻って取得の開始より前になったときは、期限切れとして取り直す
  const now = new Date().getTime();
  const cached = snippetCache.get(key);
  if (
    cached !== undefined &&
    now >= cached.startedAt &&
    now - cached.startedAt < SNIPPET_CACHE_TTL_MS
  ) {
    return cached.pending;
  }
  if (snippetCache.size >= SNIPPET_CACHE_MAX) snippetCache.clear();
  const pending = fetchSnippet(target);
  snippetCache.set(key, { pending, startedAt: now });
  // 失敗は覚えない (権限が付いた後・通信が戻った後に、次の表示で取り直せるようにする)
  pending.catch(() => {
    if (snippetCache.get(key)?.pending === pending) snippetCache.delete(key);
  });
  return pending;
}

/** 着色なしの `<kfm-code>`。行は textContent で入れる (HTML として解釈させない) */
function buildPlainCode(lines: readonly string[]): HTMLElement {
  const code = document.createElement('code');
  for (const line of lines) {
    const span = document.createElement('span');
    span.className = 'pl-line';
    span.textContent = `${line}\n`;
    code.append(span);
  }
  const pre = document.createElement('pre');
  pre.append(code);
  const wrapper = document.createElement('kfm-code');
  wrapper.append(pre);
  return wrapper;
}

/**
 * 着色済み HTML から `<kfm-code>` を取り出す。形が違えば null。
 * 行数が抜粋と合わないときも null にする: Markdown は行の途中の `\r` も改行と読むので、
 * そういう行を含む抜粋は着色後の行が増え、行番号がファイルとずれる。
 */
function extractHighlightedCode(html: string, lineCount: number): HTMLElement | null {
  const template = document.createElement('template');
  template.innerHTML = html;
  const wrapper = template.content.querySelector<HTMLElement>('kfm-code');
  const code = wrapper?.querySelector('pre > code');
  if (!wrapper || !code) return null;
  return code.querySelectorAll(':scope > .pl-line').length === lineCount ? wrapper : null;
}

function rangeLabel(target: GithubPermalink, snippet: LoadedSnippet): string {
  // 打ち切ったときは求めた範囲を見せ、出した行数を添える。そうでなければ実際の範囲
  // (ファイルの行数で丸めた結果) を見せる
  const end = snippet.truncated ? target.end : snippet.endLine;
  const range =
    end === snippet.startLine ? `${snippet.startLine} 行目` : `${snippet.startLine}〜${end} 行目`;
  const suffix = snippet.truncated ? `（先頭 ${snippet.lines.length} 行）` : '';
  return `${range}・${target.sha.slice(0, 7)}${suffix}`;
}

export function createKfmPermalinkElement(): CustomElementConstructor {
  return class KfmPermalinkElement extends HTMLElement {
    /** connectedCallback は移動・再接続でも複数回呼ばれる。取得は初回のみ */
    #started = false;
    /** 切断前に始まった取得を、再接続後の要素へ反映させないための世代 */
    #renderToken = 0;

    connectedCallback(): void {
      if (this.#started) return;
      this.#started = true;
      void this.#render(++this.#renderToken);
    }

    disconnectedCallback(): void {
      if (this.dataset.kfmPermalink === undefined) {
        this.#started = false;
        this.#renderToken += 1;
      }
    }

    async #render(token: number): Promise<void> {
      const link = this.querySelector<HTMLAnchorElement>(':scope > a[href]');
      const href = link?.getAttribute('href');
      // サーバと同じ規則で読む。読めなければ何もしない (リンクのまま)
      const target = link && href ? parseGithubPermalink(href) : null;
      if (link === null || target === null) return;

      let snippet: LoadedSnippet;
      try {
        snippet = await loadSnippet(target);
      } catch {
        if (token !== this.#renderToken || !this.isConnected) return;
        this.dataset.kfmPermalink = 'unavailable' satisfies KfmPermalinkState;
        return;
      }
      if (token !== this.#renderToken || !this.isConnected) return;

      const path = link.cloneNode(false) as HTMLAnchorElement;
      path.className = 'kfm-permalink-path';
      path.textContent = `${target.owner}/${target.repo}/${target.path}`;
      const range = document.createElement('span');
      range.className = 'kfm-permalink-range';
      range.textContent = rangeLabel(target, snippet);
      const header = document.createElement('div');
      header.className = 'kfm-permalink-header';
      header.append(path, range);

      const body =
        (snippet.html === null
          ? null
          : extractHighlightedCode(snippet.html, snippet.lines.length)) ??
        buildPlainCode(snippet.lines);
      // 行番号を抜粋の最初の行から振る。rehype-kfm-code/style.css の counter を CSSOM で
      // 上書きする (style 属性を HTML として出さない)
      body
        .querySelector<HTMLElement>('pre > code')
        ?.style.setProperty('counter-reset', `kfm-code-line ${snippet.startLine - 1}`);

      this.replaceChildren(header, body);
      this.dataset.kfmPermalink = 'rendered' satisfies KfmPermalinkState;
    }
  };
}

// KFM_PERMALINK_TAG を経由しない直書きを防ぐための再輸出 (kfm-code/element.ts と同型)
void KFM_PERMALINK_TAG;
