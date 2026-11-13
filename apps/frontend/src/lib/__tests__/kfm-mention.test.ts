// メンションのチップ（マーカーの形、除外ノード、状態、link、アバター）
import { MENTION_CLASS_NAMES } from '@koyori-app/ui-vue';
import { describe, expect, it } from 'vitest';
import type { MentionTable } from '../comment-mention';
import { renderDescription } from '../markup-renderer';
import { kfmMentionSanitizeSchema } from '../remark-kfm-mention';

const A = '11111111-1111-4111-8111-111111111111';
const B = '22222222-2222-4222-8222-222222222222';
const C = '33333333-3333-4333-8333-333333333333';
const UNKNOWN_ID = '44444444-4444-4444-8444-444444444444';
const GONE_ID = '55555555-5555-4555-8555-555555555555';

const mentions: MentionTable = {
  [A]: { status: 'member', name: 'alice' },
  // 外れた者でも名前が引けておれば名で出る（灰色の札）
  [B]: { status: 'outside', name: 'former' },
  [C]: { status: 'outside', name: null },
};

// table が null なら表を渡さない（既定の表を使うのは undefined の時だけ）
const render = (text: string, table: MentionTable | null = mentions) =>
  renderDescription(text, table === null ? {} : { mentions: table });

// 札は `@名` を span で包んだ 1 つの要素（koyori の固定 class の形。アバターは出さない）
const chip = (status: string, label: string) =>
  `<span class="koyori-mention koyori-mention-${status}"><span class="koyori-mention-label">${label}</span></span>`;

describe('remark-kfm-mention (<@利用者ID> の印を札にする)', () => {
  it('印を札にする（id は HTML に出ず、状態は class で運ぶ）', async () => {
    const html = await render(`見て <@${A}> と <@${B}> へ`);
    expect(html).toBe(
      `<p>見て ${chip('member', '@alice')} と ${chip('outside', '@former')} へ</p>`,
    );
    expect(html).not.toContain(A);
    expect(html).not.toContain('&lt;@');
  });

  it('名が引けぬ外れた者・表に無い id は定型の句（id は出ない）', async () => {
    const html = await render(`<@${C}> <@44444444-4444-4444-8444-444444444444>`);
    expect(html).toContain(chip('outside', '@（このプロジェクトの外の利用者）'));
    expect(html).toContain(chip('unknown', '@（不明な利用者）'));
    expect(html).not.toMatch(/[0-9a-f]{8}-[0-9a-f]{4}-/);
  });

  it('行の無い者は gone の札', async () => {
    const html = await render(`<@${A}>`, { [A]: { status: 'gone', name: null } });
    expect(html).toBe(`<p>${chip('gone', '@（退会した利用者）')}</p>`);
  });

  it('ログイン中の member への札だけを強調し、キャッシュでも他の閲覧者と混ぜない', async () => {
    const source = `<@${A}> <@${B}>`;
    const self = await renderDescription(source, {
      scope: 'comment-1',
      profile: 'comment',
      mentions,
      currentUserId: A,
    });
    const otherViewer = await renderDescription(source, {
      scope: 'comment-1',
      profile: 'comment',
      mentions,
      currentUserId: B,
    });
    expect(self).toContain('koyori-mention-member koyori-mention-self');
    expect(otherViewer).not.toContain('koyori-mention-member koyori-mention-self');
    expect(self).not.toBe(otherViewer);
  });

  it('強調・リスト・見出しの内側でも札にする', async () => {
    const alice = chip('member', '@alice');
    expect(await render(`**<@${A}>**`)).toBe(`<p><strong>${alice}</strong></p>`);
    expect(await render(`- <@${A}>`)).toContain(alice);
    expect(await render(`# <@${A}>`)).toContain(alice);
  });

  it('コードブロックとインラインコードの中の印は札にせず、現在の名前へ戻す', async () => {
    const inline = await render(`\`<@${A}>\``);
    expect(inline).toBe('<p><code>@alice</code></p>');
    expect(inline).not.toContain('koyori-mention');
    const block = await render(`\`\`\`\n<@${A}>\n\`\`\``);
    expect(block).not.toContain('koyori-mention');
    expect(block).toContain('@alice');
    expect(block).not.toContain(A);
  });

  it('リンクの入れ子の文字は札にせず、現在の名前へ戻す', async () => {
    const inline = await render(`[**<@${A}>**](https://example.com)`);
    expect(inline).toContain('<a href="https://example.com"><strong>@alice</strong></a>');
    expect(inline).not.toContain('koyori-mention');
    expect(inline).not.toContain(A);

    const titled = await render(`[<@${A}>](https://example.com "担当 <@${B}>")`);
    expect(titled).toContain('title="担当 @former"');
    expect(titled).toContain('>@alice</a>');
    expect(titled).not.toContain(A);
    expect(titled).not.toContain(B);

    const reference = await render(
      `[<@${A}>][person]\n\n[person]: https://example.com "担当 <@${B}>"`,
    );
    expect(reference).toContain('title="担当 @former"');
    expect(reference).toContain('>@alice</a>');
    expect(reference).not.toContain(A);
    expect(reference).not.toContain(B);
  });

  it('画像と参照画像の alt・title は現在の名前へ戻す', async () => {
    const inline = await render(`![<@${A}>](https://example.com/a.png "担当 <@${B}>")`);
    expect(inline).toContain('alt="@alice"');
    expect(inline).toContain('title="担当 @former"');
    expect(inline).not.toContain(A);
    expect(inline).not.toContain(B);

    const reference = await render(
      `![<@${A}>][avatar]\n\n[avatar]: https://example.com/a.png "担当 <@${B}>"`,
    );
    expect(reference).toContain('alt="@alice"');
    expect(reference).toContain('title="担当 @former"');
    expect(reference).not.toContain(A);
    expect(reference).not.toContain(B);
  });

  it('コードフェンスの title 属性は現在の名前へ戻す', async () => {
    const html = await render(`\`\`\`ts title="担当 <@${A}>"\nconst n = 1;\n\`\`\``);
    expect(html).toContain('data-title="担当 @alice"');
    expect(html).not.toContain(A);
  });

  it('壊れた印・手で打った @名前・メールアドレスは素の文字', async () => {
    for (const text of [
      `<@${A}`,
      `<@${A.slice(0, 35)}>`,
      `<@${A.replaceAll('-', '')}>`,
      '@alice さん',
      'bob@example.com',
    ]) {
      expect(await render(text), text).not.toContain('koyori-mention');
    }
    expect(await render('@alice さん')).toBe('<p>@alice さん</p>');
  });

  it('表を渡さぬ描画は何も変えない（印は素の文字のまま。他の消費側の描き方を変えない）', async () => {
    const html = await render(`<@${A}>`, null);
    expect(html).not.toContain('koyori-mention');
    expect(html).toContain(A);
  });

  it('表が替われば別の HTML（キャッシュが札の字を取り違えない）', async () => {
    const first = await render(`<@${A}>`, { [A]: { status: 'member', name: 'alice' } });
    const renamed = await render(`<@${A}>`, { [A]: { status: 'member', name: 'alice_renamed' } });
    const left = await render(`<@${A}>`, { [A]: { status: 'outside', name: 'alice' } });
    expect(first).toContain('@alice</span>');
    expect(renamed).toContain('@alice_renamed');
    expect(left).toContain('koyori-mention-outside');
    expect(first).not.toBe(renamed);
    expect(first).not.toBe(left);
  });

  it('名前の中の HTML は escape される（札の字は text）', async () => {
    const html = await render(`<@${A}>`, {
      [A]: { status: 'member', name: '<img src=x onerror=alert(1)>' },
    });
    expect(html).not.toContain('<img');
    expect(html).toContain('&lt;img');
  });
  it('avatar_url を渡しても札に画像・頭文字・aria-hidden を出さない', async () => {
    for (const url of [
      'https://cdn.example.com/a.png',
      'javascript:alert(1)',
      '" onerror="alert(1)',
    ]) {
      const html = await render(`<@${A}> <@${B}>`, {
        [A]: { status: 'member', name: 'alice', avatar_url: url },
        [B]: { status: 'outside', name: 'former', avatar_url: url },
      });
      expect(html, url).not.toContain('<img');
      expect(html, url).not.toContain('data-initials');
      expect(html, url).not.toContain('aria-hidden');
      expect(html, url).not.toContain('onerror');
    }
  });

  it('link の札の子は字の span 1 つだけ', async () => {
    const html = await renderDescription(`<@${A}>`, { mentions, mentionTenant: 'acme' });
    const doc = new DOMParser().parseFromString(html, 'text/html');
    const a = doc.querySelector('a.koyori-mention');
    expect(a).not.toBeNull();
    expect(a?.childNodes).toHaveLength(1);
    expect(a?.innerHTML).toBe('<span class="koyori-mention-label">@alice</span>');
  });

  it('表の status が知らない値なら unknown の札（値は class に出ず、self も付かない）', async () => {
    const html = await renderDescription(`<@${A}>`, {
      mentions: { [A]: { status: 'bot' as never, name: 'alice' } },
      currentUserId: A,
    });
    expect(html).toContain('class="koyori-mention koyori-mention-unknown"');
    expect(html).not.toContain('bot');
    expect(html).not.toContain('koyori-mention-self');
  });

  it('許可リストは koyori の固定 class と過不足なく一致する', () => {
    expect([...kfmMentionSanitizeSchema.classTokens].sort()).toEqual(
      [...MENTION_CLASS_NAMES].sort(),
    );
  });

  describe('札のプロフィールへの link', () => {
    const ORIGIN = 'https://task.example';
    const withTenant = (text: string, tenant: string | undefined, table: MentionTable = mentions) =>
      renderDescription(text, { mentions: table, mentionTenant: tenant });
    /** HTML の中の a を DOM で読み直し、href を（同じ origin の上で解いた形で）返す */
    const links = (html: string) => {
      const doc = new DOMParser().parseFromString(html, 'text/html');
      return [...doc.querySelectorAll('a')].map((a) => ({
        raw: a.getAttribute('href') ?? '',
        url: new URL(a.getAttribute('href') ?? '', ORIGIN),
        className: a.className,
        text: a.textContent,
      }));
    };

    it('いま入れる者の札はテナントのプロフィールへの link（道は id とテナントだけ）', async () => {
      const html = await withTenant(`見て <@${A}>`, 'acme');
      const [link] = links(html);
      expect(link.raw).toBe(`/acme/users/${A}`);
      expect(link.className).toBe('koyori-mention koyori-mention-member');
      expect(link.text).toBe('@alice');
      expect(link.url.origin).toBe(ORIGIN);
    });

    it('テナントを渡さぬ描画は今どおり span（link にせぬ）', async () => {
      const html = await render(`<@${A}>`);
      expect(html).not.toContain('<a');
      expect(html).toContain('<span class="koyori-mention koyori-mention-member">');
    });

    it('外れた者・名の引けぬ者・退会した者・表に無い者の札は link にせぬ', async () => {
      const html = await withTenant(`<@${B}> <@${C}> <@${UNKNOWN_ID}> <@${GONE_ID}>`, 'acme', {
        ...mentions,
        [GONE_ID]: { status: 'gone', name: null },
      });
      expect(links(html)).toHaveLength(0);
      expect(html.match(/class="koyori-mention /g)).toHaveLength(4);
    });

    it('字種外のテナントでは link にせず、他の scheme・他の origin へ向かう href を作らぬ', async () => {
      for (const tenant of [
        'evil.example.com',
        '//evil.example.com',
        'javascript:alert(1)',
        '../admin',
        'acme/users',
        'acme"><img src=x>',
        '',
        '-acme',
      ]) {
        const html = await withTenant(`<@${A}>`, tenant);
        expect(links(html), tenant).toHaveLength(0);
        expect(html, tenant).not.toContain('evil');
        expect(html, tenant).not.toContain('javascript:');
      }
    });

    it('本文の字や link の記法で札の link の先を替えられぬ', async () => {
      const html = await withTenant(
        `[<@${A}>](https://evil.example.com) <@${A}> [x](javascript:alert(1)) <@${A}>`,
        'acme',
      );
      for (const link of links(html).filter((l) => l.className.includes('koyori-mention'))) {
        expect(link.raw).toBe(`/acme/users/${A}`);
        expect(link.url.origin).toBe(ORIGIN);
        expect(link.url.protocol).toBe('https:');
      }
      // link の記法の内の印は札にせぬ（今どおり）
      expect(links(html).filter((l) => l.className.includes('koyori-mention'))).toHaveLength(2);
    });

    it('本人への札も link（self の強調を保つ）', async () => {
      const html = await renderDescription(`<@${A}>`, {
        mentions,
        mentionTenant: 'acme',
        currentUserId: A,
      });
      expect(links(html)[0].className).toBe(
        'koyori-mention koyori-mention-member koyori-mention-self',
      );
    });

    it('テナントが替われば別の HTML（キャッシュが link の先を取り違えない）', async () => {
      const first = await withTenant(`<@${A}>`, 'acme');
      const second = await withTenant(`<@${A}>`, 'other');
      expect(links(first)[0].raw).toBe(`/acme/users/${A}`);
      expect(links(second)[0].raw).toBe(`/other/users/${A}`);
    });
  });
});
