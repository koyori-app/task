// GitHub のパーマリンクをコードの抜粋として出す（URL の解釈、remark 層の emit、custom element の取得と差し込み）
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest';
import { renderDescription } from '../markup-renderer';
import { createKfmCodeElement, KFM_CODE_TAG } from '../rehype-kfm-code/element';
import {
  buildSnippetFence,
  KFM_PERMALINK_TAG,
  languageFlagForPath,
  parseGithubPermalink,
} from '../remark-kfm-permalink';
import { createKfmPermalinkElement } from '../remark-kfm-permalink/element';

const SHA = 'a3f92c1e7b81d4000000000000000000000000aa';
const BASE = `https://github.com/acme/backend/blob/${SHA}`;

describe('parseGithubPermalink', () => {
  it.each([
    [`${BASE}/src/a.rs#L3`, { path: 'src/a.rs', start: 3, end: 3 }],
    [`${BASE}/src/a.rs#L3-L10`, { path: 'src/a.rs', start: 3, end: 10 }],
    // 列は捨てる
    [`${BASE}/src/a.rs#L3C1-L10C5`, { path: 'src/a.rs', start: 3, end: 10 }],
    [`${BASE}/src/a.rs?plain=1#L1-L2`, { path: 'src/a.rs', start: 1, end: 2 }],
    // path は区分ごとに復号する
    [`${BASE}/docs/a%20b/c.md#L1`, { path: 'docs/a b/c.md', start: 1, end: 1 }],
    [
      `${BASE}/docs/%E6%97%A5%E6%9C%AC%E8%AA%9E.md#L2`,
      { path: 'docs/日本語.md', start: 2, end: 2 },
    ],
    [`${BASE}/Dockerfile#L7-L7`, { path: 'Dockerfile', start: 7, end: 7 }],
  ])('%s を読む', (href, expected) => {
    expect(parseGithubPermalink(href)).toEqual({
      owner: 'acme',
      repo: 'backend',
      sha: SHA,
      ...expected,
    });
  });

  it('SHA は小文字にそろえる', () => {
    const href = `https://github.com/Acme/Back.end/blob/${SHA.toUpperCase()}/a.rs#L1`;
    expect(parseGithubPermalink(href)).toEqual({
      owner: 'Acme',
      repo: 'Back.end',
      sha: SHA,
      path: 'a.rs',
      start: 1,
      end: 1,
    });
  });

  it.each([
    ['ブランチ名', 'https://github.com/acme/backend/blob/main/src/a.rs#L3'],
    ['短縮 SHA', `https://github.com/acme/backend/blob/${SHA.slice(0, 7)}/src/a.rs#L3`],
    ['行の指定なし', `${BASE}/src/a.rs`],
    ['行でない fragment', `${BASE}/README.md#usage`],
    ['end < start', `${BASE}/src/a.rs#L10-L3`],
    ['L0', `${BASE}/src/a.rs#L0`],
    ['8 桁の行', `${BASE}/src/a.rs#L12345678`],
    ['http', `http://github.com/acme/backend/blob/${SHA}/src/a.rs#L3`],
    ['別ホスト', `https://gist.github.com/acme/backend/blob/${SHA}/src/a.rs#L3`],
    ['似たホスト', `https://github.com.example.com/acme/backend/blob/${SHA}/src/a.rs#L3`],
    ['ポートつき', `https://github.com:8443/acme/backend/blob/${SHA}/src/a.rs#L3`],
    ['認証情報つき', `https://user@github.com/acme/backend/blob/${SHA}/src/a.rs#L3`],
    ['.. を含む path', `${BASE}/src/../a.rs#L3`],
    ['符号化した ..', `${BASE}/src/%2e%2e/a.rs#L3`],
    ['符号化した /', `${BASE}/src%2Fa.rs/b#L3`],
    ['. の区分', `${BASE}/./a.rs#L3`],
    ['空の区分', `${BASE}/src//a.rs#L3`],
    ['壊れた符号化', `${BASE}/src/%E0%A4%A.rs#L3`],
    ['制御文字', `${BASE}/src/a%0A.rs#L3`],
    ['逆スラッシュ', `${BASE}/src\\a.rs#L3`],
    ['余分な query', `${BASE}/src/a.rs?plain=1&x=1#L3`],
    ['別の query', `${BASE}/src/a.rs?raw=true#L3`],
    ['blob でない', `https://github.com/acme/backend/tree/${SHA}/src#L3`],
    ['path なし', `https://github.com/acme/backend/blob/${SHA}#L3`],
    ['owner の字種', `https://github.com/-acme/backend/blob/${SHA}/a.rs#L3`],
    ['repo が ..', `https://github.com/acme/../blob/${SHA}/a.rs#L3`],
    ['長すぎる path', `${BASE}/${'a/'.repeat(600)}a.rs#L3`],
    ['URL でない', 'not a url'],
  ])('%s は null', (_label, href) => {
    expect(parseGithubPermalink(href)).toBeNull();
  });
});

describe('languageFlagForPath', () => {
  it.each([
    ['src/a.rs', 'rs'],
    ['Dockerfile', 'dockerfile'],
    ['a/b.C++', 'c++'],
    ['a.b/Makefile', 'makefile'],
    ['.github/workflows/ci.yml', 'yml'],
  ])('%s → %s', (path, flag) => {
    expect(languageFlagForPath(path)).toBe(flag);
  });

  it.each(['docs/日本語', 'a/b.c d', 'a/b.`', 'trailing.'])('%s は字種外で undefined', (path) => {
    expect(languageFlagForPath(path)).toBeUndefined();
  });
});

describe('remark-kfm-permalink (SSR / remark 層)', () => {
  const url = `${BASE}/src/a.rs#L3-L10`;

  /** 描画結果の中の kfm-permalink 要素（無ければ空） */
  function permalinksOf(html: string): HTMLElement[] {
    const template = document.createElement('template');
    template.innerHTML = html;
    return Array.from(template.content.querySelectorAll<HTMLElement>(KFM_PERMALINK_TAG));
  }

  it('段落が URL だけなら <kfm-permalink> がリンクを包む (属性は出さない)', async () => {
    const html = await renderDescription(`前の段落\n\n${url}\n\n後の段落`);
    const [permalink, ...rest] = permalinksOf(html);
    expect(rest).toHaveLength(0);
    expect(permalink?.attributes).toHaveLength(0);
    const link = permalink?.querySelector(':scope > a');
    expect(link?.getAttribute('href')).toBe(url);
    expect(link?.textContent).toBe(url);
    // 段落の <p> は残らない (kfm-permalink が段落の位置に入る)
    expect(permalink?.parentElement).toBeNull();
    expect(html).toContain('<p>前の段落</p>');
  });

  it.each([
    ['<URL> の autolink', `<${url}>`],
    ['リスト項目の中', `- 項目\n\n  ${url}\n`],
    ['引用の中', `> ${url}`],
    ['?plain=1 つき', `${BASE}/README.md?plain=1#L1-L2`],
  ])('%s でも抜粋にする', async (_label, source) => {
    expect(permalinksOf(await renderDescription(source))).toHaveLength(1);
  });

  it('comment profile でも同じ', async () => {
    const html = await renderDescription(`一行目\n二行目\n\n${url}`, { profile: 'comment' });
    expect(permalinksOf(html)).toHaveLength(1);
    expect(html).toContain('<br>');
  });

  it.each([
    ['[文字](URL)', `[ここ](${url})`],
    ['文中の URL', `ここを見て ${url} ください`],
    ['同じ段落に 2 つ', `${url} ${url}`],
    ['改行で続く文', `${url}\n続きの文`],
    ['ブランチ名の URL', 'https://github.com/acme/backend/blob/main/src/a.rs#L3'],
    ['行の指定が無い URL', `${BASE}/src/a.rs`],
    ['コードフェンスの中', `\`\`\`\n${url}\n\`\`\``],
    ['インラインコードの中', `\`${url}\``],
    ['見出し', `# ${url}`],
  ])('%s は抜粋にしない (リンク・文字のまま)', async (_label, source) => {
    const html = await renderDescription(source);
    expect(html).not.toContain(KFM_PERMALINK_TAG);
  });

  it('本文に生で書いた <kfm-permalink> は要素にならず文字として出る', async () => {
    const html = await renderDescription(
      `<kfm-permalink data-kfm-permalink="rendered"><a href="${url}">x</a></kfm-permalink>`,
    );
    expect(permalinksOf(html)).toHaveLength(0);
    expect(html).toContain('&lt;kfm-permalink');
  });
});

describe('buildSnippetFence (描画器との突き合わせ)', () => {
  it('バッククォートの連続より長いフェンスで包む', () => {
    expect(buildSnippetFence(['a'], 'rs')).toBe('```rs\na\n```');
    expect(buildSnippetFence(['a'])).toBe('```\na\n```');
    expect(buildSnippetFence(['```', 'x ````` y'], 'md')).toBe('``````md\n```\nx ````` y\n``````');
  });

  /** 種を固定した擬似乱数 (mulberry32)。試験を決定的にする */
  function createRandom(seed: number): () => number {
    let state = seed;
    return () => {
      state = (state + 0x6d2b79f5) | 0;
      let t = Math.imul(state ^ (state >>> 15), 1 | state);
      t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
      return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
    };
  }

  it('どんな行の列でも、描画した行が入力と一致する (フェンスが破れない)', async () => {
    const random = createRandom(482);
    // Markdown の構文になる字 (フェンス・見出し・引用・リスト・HTML) と空白に寄せる
    const alphabet = ['`', '`', '`', '~', '~', '#', '>', '-', ' ', ' ', 'a', '<', '&', '*', '\\'];
    const pick = (max: number) => Math.floor(random() * max);
    for (let round = 0; round < 300; round += 1) {
      const lines = Array.from({ length: 1 + pick(6) }, () =>
        // 4 回に 1 回は空行 (先頭・末尾の空行を含む)
        pick(4) === 0
          ? ''
          : Array.from({ length: 1 + pick(8) }, () => alphabet[pick(alphabet.length)]).join(''),
      );
      const html = await renderDescription(buildSnippetFence(lines, 'ts'), { scope: 'permalink' });
      const template = document.createElement('template');
      template.innerHTML = html;
      const rendered = Array.from(template.content.querySelectorAll('.pl-line')).map((line) =>
        (line.textContent ?? '').replace(/\n$/, ''),
      );
      expect(rendered, JSON.stringify(lines)).toEqual(lines);
      // 抜粋の外へ漏れた要素が無い (描画結果は kfm-code 1 つだけ)
      expect(template.content.children).toHaveLength(1);
      expect(template.content.firstElementChild?.localName).toBe('kfm-code');
    }
  }, 60_000);
});

describe('KfmPermalinkElement (client 層)', () => {
  beforeAll(() => {
    if (customElements.get(KFM_PERMALINK_TAG) === undefined) {
      customElements.define(KFM_PERMALINK_TAG, createKfmPermalinkElement());
    }
    if (customElements.get(KFM_CODE_TAG) === undefined) {
      customElements.define(KFM_CODE_TAG, createKfmCodeElement());
    }
  });

  afterEach(() => {
    document.body.innerHTML = '';
    vi.unstubAllGlobals();
  });

  /** 抜粋は module の中で覚えられるので、試験ごとに別の SHA を使う */
  let shaSequence = 0;
  function nextSha(): string {
    shaSequence += 1;
    return shaSequence.toString(16).padStart(40, '0');
  }

  function permalinkUrl(sha: string, fragment: string): string {
    return `https://github.com/acme/backend/blob/${sha}/src/a.rs${fragment}`;
  }

  /** SSR 出力相当の light DOM で接続する */
  function mount(href: string): HTMLElement {
    const element = document.createElement(KFM_PERMALINK_TAG);
    const link = document.createElement('a');
    link.href = href;
    link.textContent = href;
    element.append(link);
    document.body.append(element);
    return element;
  }

  function json(status: number, body: unknown): Response {
    return new Response(JSON.stringify(body), {
      status,
      headers: { 'Content-Type': 'application/json' },
    });
  }

  function snippetBody(sha: string, startLine: number, lines: string[], truncated = false) {
    return {
      repo_owner: 'acme',
      repo_name: 'backend',
      sha,
      path: 'src/a.rs',
      start_line: startLine,
      end_line: startLine + lines.length - 1,
      lines,
      truncated,
    };
  }

  type Handler = (url: string, init?: RequestInit) => Response | Promise<Response>;

  /** 抜粋と着色の口を差し替える。返すのは受けた URL の列 */
  function stubFetch(snippet: Handler, highlight: Handler): string[] {
    const calls: string[] = [];
    vi.stubGlobal(
      'fetch',
      vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
        const url = String(input);
        calls.push(url);
        if (url.startsWith('/api/v1/github/snippet?')) return snippet(url, init);
        if (url === '/internal/render-description') return highlight(url, init);
        throw new Error(`unexpected fetch: ${url}`);
      }),
    );
    return calls;
  }

  /** 着色の口の代わりに、本番と同じ描画器で送られた本文を描く */
  const renderHighlight: Handler = async (_url, init) => {
    const { description, scope } = JSON.parse(String(init?.body)) as {
      description: string;
      scope: string;
    };
    return json(200, { html: await renderDescription(description, { scope }) });
  };

  function linesOf(element: HTMLElement): string[] {
    return Array.from(element.querySelectorAll('.pl-line')).map((line) => line.textContent ?? '');
  }

  it('抜粋が取れたら、ヘッダ (path・行・SHA 7 桁) と着色済みの kfm-code に差し替える', async () => {
    const sha = nextSha();
    const href = permalinkUrl(sha, '#L3-L4');
    const calls = stubFetch(
      () => json(200, snippetBody(sha, 3, ['fn main() {', '}'])),
      renderHighlight,
    );
    const element = mount(href);
    await vi.waitFor(() => expect(element.dataset.kfmPermalink).toBe('rendered'));

    // backend へは分解した値を送る
    const query = new URL(calls[0] ?? '', 'http://localhost').searchParams;
    expect(Object.fromEntries(query)).toEqual({
      owner: 'acme',
      repo: 'backend',
      sha,
      path: 'src/a.rs',
      start: '3',
      end: '4',
    });

    const path = element.querySelector<HTMLAnchorElement>('.kfm-permalink-header > a');
    expect(path?.className).toBe('kfm-permalink-path');
    expect(path?.getAttribute('href')).toBe(href);
    expect(path?.textContent).toBe('acme/backend/src/a.rs');
    expect(element.querySelector('.kfm-permalink-range')?.textContent).toBe(
      `3〜4 行目・${sha.slice(0, 7)}`,
    );

    const code = element.querySelector<HTMLElement>(':scope > kfm-code');
    expect(code?.dataset.lang).toBe('rs');
    expect(linesOf(element)).toEqual(['fn main() {\n', '}\n']);
    // 着色されている (starry-night の class が付く)
    expect(code?.querySelector('.pl-k')).not.toBeNull();
    // 行番号は抜粋の最初の行から
    expect(code?.querySelector<HTMLElement>('pre > code')?.style.counterReset).toBe(
      'kfm-code-line 2',
    );
    // 元のリンクは残さない (ヘッダのリンクに替わる)
    expect(element.querySelectorAll('a')).toHaveLength(1);
    // コピーのボタンは差し込んだ kfm-code が自分で足す
    expect(code?.querySelector('.kfm-code-copy')).not.toBeNull();
  });

  it('1 行の抜粋と、打ち切られた抜粋の行の表示', async () => {
    const single = nextSha();
    const long = nextSha();
    stubFetch(
      (url) =>
        url.includes(single)
          ? json(200, snippetBody(single, 7, ['one']))
          : json(200, snippetBody(long, 1, ['a', 'b', 'c'], true)),
      renderHighlight,
    );
    const one = mount(permalinkUrl(single, '#L7'));
    const cut = mount(permalinkUrl(long, '#L1-L900'));
    await vi.waitFor(() => {
      expect(one.dataset.kfmPermalink).toBe('rendered');
      expect(cut.dataset.kfmPermalink).toBe('rendered');
    });
    expect(one.querySelector('.kfm-permalink-range')?.textContent).toBe(
      `7 行目・${single.slice(0, 7)}`,
    );
    expect(cut.querySelector('.kfm-permalink-range')?.textContent).toBe(
      `1〜900 行目・${long.slice(0, 7)}（先頭 3 行）`,
    );
  });

  it('抜粋が 404 ならリンクをそのまま残す (対照: 200 なら差し替わる)', async () => {
    const denied = nextSha();
    const allowed = nextSha();
    const calls = stubFetch(
      (url) =>
        url.includes(denied)
          ? json(404, { type: 'not-found' })
          : json(200, snippetBody(allowed, 1, ['ok'])),
      renderHighlight,
    );
    const href = permalinkUrl(denied, '#L1');
    const refused = mount(href);
    const shown = mount(permalinkUrl(allowed, '#L1'));
    await vi.waitFor(() => {
      expect(refused.dataset.kfmPermalink).toBe('unavailable');
      expect(shown.dataset.kfmPermalink).toBe('rendered');
    });
    expect(refused.children).toHaveLength(1);
    expect(refused.querySelector(':scope > a')?.getAttribute('href')).toBe(href);
    expect(refused.textContent).toBe(href);
    // 取れなかった抜粋は着色へ送らない
    expect(calls.filter((url) => url === '/internal/render-description')).toHaveLength(1);
    expect(linesOf(shown)).toEqual(['ok\n']);
  });

  it.each([
    ['500', () => json(500, {})],
    ['401', () => new Response('Unauthorized', { status: 401 })],
    ['html が null', () => json(200, { html: null })],
    ['kfm-code を含まぬ HTML', () => json(200, { html: '<p>x</p>' })],
    [
      '通信の失敗',
      () => {
        throw new TypeError('network');
      },
    ],
  ] satisfies [string, Handler][])(
    '着色が %s のときは無着色で出し、行を HTML として解釈しない',
    async (_label, highlight) => {
      const sha = nextSha();
      const lines = ['<img src=x onerror="alert(1)">', 'a < b && c > d', ''];
      stubFetch(() => json(200, snippetBody(sha, 10, lines)), highlight);
      const element = mount(permalinkUrl(sha, '#L10-L12'));
      await vi.waitFor(() => expect(element.dataset.kfmPermalink).toBe('rendered'));

      expect(linesOf(element)).toEqual(lines.map((line) => `${line}\n`));
      expect(element.querySelector('img')).toBeNull();
      expect(element.querySelector('p')).toBeNull();
      const code = element.querySelector<HTMLElement>(':scope > kfm-code > pre > code');
      expect(code?.style.counterReset).toBe('kfm-code-line 9');
      expect(element.querySelector('.kfm-permalink-range')?.textContent).toBe(
        `10〜12 行目・${sha.slice(0, 7)}`,
      );
    },
  );

  it('同じパーマリンクが 2 つあっても、抜粋の取得は 1 回', async () => {
    const sha = nextSha();
    const calls = stubFetch(() => json(200, snippetBody(sha, 1, ['x'])), renderHighlight);
    const first = mount(permalinkUrl(sha, '#L1'));
    const second = mount(permalinkUrl(sha, '#L1'));
    await vi.waitFor(() => {
      expect(first.dataset.kfmPermalink).toBe('rendered');
      expect(second.dataset.kfmPermalink).toBe('rendered');
    });
    expect(calls.filter((url) => url.startsWith('/api/v1/github/snippet?'))).toHaveLength(1);
    expect(calls.filter((url) => url === '/internal/render-description')).toHaveLength(1);
    // 2 つとも自分の kfm-code を持つ (同じノードを取り合わない)
    expect(linesOf(first)).toEqual(['x\n']);
    expect(linesOf(second)).toEqual(['x\n']);
  });

  it('失敗した取得は覚えず、次の表示で取り直す', async () => {
    const sha = nextSha();
    let allowed = false;
    const calls = stubFetch(
      () => (allowed ? json(200, snippetBody(sha, 1, ['x'])) : json(404, {})),
      renderHighlight,
    );
    const first = mount(permalinkUrl(sha, '#L1'));
    await vi.waitFor(() => expect(first.dataset.kfmPermalink).toBe('unavailable'));
    allowed = true;
    const second = mount(permalinkUrl(sha, '#L1'));
    await vi.waitFor(() => expect(second.dataset.kfmPermalink).toBe('rendered'));
    expect(calls.filter((url) => url.startsWith('/api/v1/github/snippet?'))).toHaveLength(2);
  });

  it('取得の途中で切断されたら、解決しても DOM を触らない', async () => {
    const sha = nextSha();
    let release: (response: Response) => void = () => undefined;
    const gate = new Promise<Response>((resolve) => {
      release = resolve;
    });
    const calls = stubFetch(() => gate, renderHighlight);
    const href = permalinkUrl(sha, '#L1');
    const element = mount(href);
    await vi.waitFor(() => expect(calls).toHaveLength(1));
    element.remove();
    release(json(200, snippetBody(sha, 1, ['x'])));
    // 着色まで終わるのを待つ (取得の続きが全部流れた後で見る)
    await vi.waitFor(() => expect(calls).toContain('/internal/render-description'));
    await new Promise((resolve) => setTimeout(resolve, 0));

    expect(element.dataset.kfmPermalink).toBeUndefined();
    expect(element.children).toHaveLength(1);
    expect(element.querySelector(':scope > a')?.getAttribute('href')).toBe(href);

    // 対照: つなぎ直せば描かれる (覚えた抜粋を使い、取り直さない)
    document.body.append(element);
    await vi.waitFor(() => expect(element.dataset.kfmPermalink).toBe('rendered'));
    expect(calls.filter((url) => url.startsWith('/api/v1/github/snippet?'))).toHaveLength(1);
  });

  it('パーマリンクでないリンクを包んでいても何もしない', async () => {
    const calls = stubFetch(
      () => json(200, {}),
      () => json(200, {}),
    );
    const element = mount('https://github.com/acme/backend/blob/main/src/a.rs#L1');
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(calls).toHaveLength(0);
    expect(element.dataset.kfmPermalink).toBeUndefined();
    expect(element.children).toHaveLength(1);
  });
});
