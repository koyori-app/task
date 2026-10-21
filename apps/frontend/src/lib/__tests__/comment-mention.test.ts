import { describe, expect, it } from 'vitest';

import {
  filterCandidates,
  findMarkers,
  fromClipboardHtml,
  lookup,
  markerOf,
  mentionChip,
  mentionLabel,
  mentionTrigger,
  safeAvatarUrl,
  toClipboardHtml,
  toPlainText,
  type MentionTable,
} from '../comment-mention';

const A = '11111111-1111-4111-8111-111111111111';
const B = '22222222-2222-4222-8222-222222222222';

const table: MentionTable = {
  [A]: { status: 'member', name: 'alice' },
  [B]: { status: 'outside', name: null },
};

describe('findMarkers / markerOf', () => {
  it('印の位置と id を前から順に返す', () => {
    const text = `見て ${markerOf(A)} と ${markerOf(B)}`;
    const found = findMarkers(text);
    expect(found.map((m) => m.id)).toEqual([A, B]);
    expect(text.slice(found[0].from, found[0].to)).toBe(`<@${A}>`);
  });

  it('壊れた印・名前の文字は拾わない', () => {
    const broken = [
      `<@${A}`,
      `<@${A.slice(0, 35)}>`,
      `<@${A.replaceAll('-', '')}>`,
      `< @${A}>`,
      '@alice bob@example.com <@alice>',
    ];
    for (const text of broken) expect(findMarkers(text), text).toEqual([]);
  });

  it('大文字の id は小文字に揃えて返す', () => {
    expect(findMarkers(`<@${A.toUpperCase()}>`)[0].id).toBe(A);
  });
});

describe('mentionLabel / toPlainText', () => {
  it('member は @今の名、他は名を伏せた定型の句。id は出さない', () => {
    expect(mentionLabel(table[A])).toBe('@alice');
    expect(mentionLabel(table[B])).toBe('@（このプロジェクトの外の利用者）');
    expect(mentionLabel({ status: 'gone' })).toBe('@（退会した利用者）');
    expect(mentionLabel(undefined)).toBe('@（不明な利用者）');
  });

  it('コピー用に印を札の字へ直す', () => {
    const plain = toPlainText(`${markerOf(A)} ${markerOf(B)}`, (id) => lookup(table, id));
    expect(plain).toBe('@alice @（このプロジェクトの外の利用者）');
    expect(plain).not.toContain(A);
  });
});

describe('mentionTrigger', () => {
  it('行頭・空白・括弧の後の @検索語 を拾う', () => {
    expect(mentionTrigger('@')).toEqual({ from: 0, query: '' });
    expect(mentionTrigger('お願い @al')).toEqual({ from: 4, query: 'al' });
    expect(mentionTrigger('（@bo')).toEqual({ from: 1, query: 'bo' });
  });

  it('メールアドレス中の @・印の中の @・空白後の語は拾わない', () => {
    expect(mentionTrigger('bob@example')).toBeNull();
    expect(mentionTrigger(`${markerOf(A)}@`)).toBeNull();
    expect(mentionTrigger('<@abc')).toBeNull();
    expect(mentionTrigger('@alice さん')).toBeNull();
  });
});

describe('filterCandidates', () => {
  const people = ['alice', 'Bob', 'malice', 'carol'].map((username) => ({
    id: username,
    username,
  }));

  it('前方一致を先に、次に部分一致。大文字小文字は無視', () => {
    expect(filterCandidates(people, 'al').map((p) => p.username)).toEqual(['alice', 'malice']);
    expect(filterCandidates(people, 'BO').map((p) => p.username)).toEqual(['Bob']);
  });

  it('検索語が空なら先頭から上限まで', () => {
    expect(filterCandidates(people, '', 2)).toHaveLength(2);
    expect(filterCandidates(people, 'zzz')).toEqual([]);
  });
});

describe('クリップボードの札（toClipboardHtml / fromClipboardHtml）', () => {
  const resolve = (id: string) =>
    id === A ? { status: 'member' as const, name: 'alice' } : undefined;

  it('書いて読むと元の文に戻る（改行・特殊字も）', () => {
    const text = `見て <@${A}> & <b>確認</b>\n次の行 <@${B}>`;
    const html = toClipboardHtml(text, resolve);
    expect(html).toContain(`data-task-mention="1" data-user-id="${A}">@alice</span>`);
    expect(html).toContain('&lt;b&gt;');
    expect(fromClipboardHtml(html)).toBe(text);
  });

  it('名の引けぬ者の表示字に id を出さない', () => {
    expect(toClipboardHtml(`<@${B}>`, resolve)).toContain('@（不明な利用者）');
  });

  it('task の印が無い HTML・知らぬ版・壊れた id は札にしない', () => {
    expect(fromClipboardHtml('<p>ただの文</p>')).toBeNull();
    expect(fromClipboardHtml(`<span data-user-id="${A}">@x</span>`)).toBeNull();
    expect(
      fromClipboardHtml(`<span data-task-mention="2" data-user-id="${A}">@x</span>`),
    ).toBeNull();
    expect(
      fromClipboardHtml('<span data-task-mention="1" data-user-id="not-a-uuid">@x</span>'),
    ).toBeNull();
  });

  it('壊れた札は字だけ拾い、正しい札は戻す', () => {
    const html = `<div><span data-task-mention="1" data-user-id="zzz">@bad</span> <span data-task-mention="1" data-user-id="${A.toUpperCase()}">@ok</span></div>`;
    expect(fromClipboardHtml(html)).toBe(`@bad <@${A}>`);
  });

  it('script・style の中身は読まない', () => {
    const html = `<style>.x{}</style><script>alert(1)</script><span data-task-mention="1" data-user-id="${A}">@a</span>`;
    expect(fromClipboardHtml(html)).toBe(`<@${A}>`);
  });
});

describe('mentionChip（札の見た目の判じ。KFM・入力欄・コメントの札が共に通る）', () => {
  const ID = '11111111-1111-4111-8111-111111111111';

  it('いま入れる者はアバター付き、本人なら self', () => {
    const user = {
      status: 'member',
      name: 'alice',
      avatar_url: 'https://cdn.example.com/a.png',
    } as const;
    expect(mentionChip(user, ID, ID.toUpperCase())).toEqual({
      state: 'member',
      self: true,
      label: '@alice',
      avatar: { name: 'alice', src: 'https://cdn.example.com/a.png', inactive: false },
    });
    expect(mentionChip(user, ID, null).self).toBe(false);
  });

  it('外れた者は灰色のアバター、本人でも self にせぬ', () => {
    const chip = mentionChip({ status: 'outside', name: 'former', avatar_url: null }, ID, ID);
    expect(chip.self).toBe(false);
    expect(chip.avatar).toEqual({ name: 'former', src: null, inactive: true });
  });

  it('名の引けぬ者・表に無い者は定型の句だけでアバター無し', () => {
    expect(mentionChip({ status: 'outside', name: null }, ID).avatar).toBeNull();
    expect(mentionChip({ status: 'gone', name: null }, ID).avatar).toBeNull();
    expect(mentionChip(undefined, ID)).toMatchObject({
      state: 'unknown',
      label: '@（不明な利用者）',
      avatar: null,
    });
  });
});

describe('safeAvatarUrl', () => {
  it('http(s) の絶対 URL と同じ origin の道だけを通す', () => {
    expect(safeAvatarUrl('https://cdn.example.com/a.png')).toBe('https://cdn.example.com/a.png');
    expect(safeAvatarUrl('http://cdn.example.com/a.png')).toBe('http://cdn.example.com/a.png');
    expect(safeAvatarUrl('/v1/avatars/a.png')).toBe('/v1/avatars/a.png');
    expect(safeAvatarUrl('data:image/png;base64,iVBORw0KGgo=')).toBe(
      'data:image/png;base64,iVBORw0KGgo=',
    );
    for (const url of [
      null,
      undefined,
      '',
      'javascript:alert(1)',
      'JAVASCRIPT:alert(1)',
      'data:image/svg+xml;base64,PHN2Zz4=',
      'data:image/svg+xml,<svg onload=alert(1)>',
      'data:text/html;base64,PHNjcmlwdD4=',
      'data:image/png;base64,AA"onerror="x',
      '//evil.example.com/a.png',
      '/\\evil.example.com/a.png',
      'a.png',
    ]) {
      expect(safeAvatarUrl(url), String(url)).toBeNull();
    }
  });
});
