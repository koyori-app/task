// メンションの札のカード（開く札の判定・対象の引き方・所属の一行）
import { describe, expect, it } from 'vitest';
import type { MentionTable } from '../comment-mention';
import {
  MENTION_CARD_SELECTOR,
  hasMentionCards,
  mentionCardTarget,
  mentionCardUserId,
  profileMetaLine,
} from '../mention-card';
import { renderDescription } from '../markup-renderer';

const A = '11111111-1111-4111-8111-111111111111';
const B = '22222222-2222-4222-8222-222222222222';
const C = '33333333-3333-4333-8333-333333333333';
const D = '44444444-4444-4444-8444-444444444444';
const E = '55555555-5555-4555-8555-555555555555';
const MISSING = '66666666-6666-4666-8666-666666666666';
const L = 'abcdef01-abcd-4bcd-8bcd-abcdef012345';

const table: MentionTable = {
  [A]: { status: 'member', name: 'alice', avatar_url: 'https://cdn.example.com/a.png' },
  [B]: { status: 'outside', name: 'former', avatar_url: null },
  [C]: { status: 'outside', name: null },
  [D]: { status: 'gone', name: null },
  [E]: { status: 'member', name: '' },
  [L]: { status: 'member', name: 'letters' },
};

describe('mentionCardUserId', () => {
  it('いま入れる者と名の引ける外れた者は id を返す', () => {
    expect(mentionCardUserId(table[A], A)).toBe(A);
    expect(mentionCardUserId(table[B], B)).toBe(B);
  });

  it('大文字の id は小文字にする', () => {
    expect(mentionCardUserId(table[L], L.toUpperCase())).toBe(L);
  });

  it('名の無い者・退会した者・表に無い者・知らない status は null', () => {
    expect(mentionCardUserId(table[C], C)).toBeNull();
    expect(mentionCardUserId(table[E], E)).toBeNull();
    expect(mentionCardUserId(table[D], D)).toBeNull();
    expect(mentionCardUserId({ status: 'gone', name: 'x' }, D)).toBeNull();
    expect(mentionCardUserId(undefined, MISSING)).toBeNull();
    expect(mentionCardUserId({ status: 'bot' as never, name: 'x' }, A)).toBeNull();
  });

  it('UUID の形でない id は null', () => {
    expect(mentionCardUserId(table[A], 'not-a-uuid')).toBeNull();
    expect(mentionCardUserId(table[A], `${A}"><x>`)).toBeNull();
  });
});

describe('mentionCardTarget', () => {
  const el = (html: string): Element => {
    const root = document.createElement('div');
    root.innerHTML = html;
    return root.firstElementChild!;
  };

  it('退会した者の札は gone', () => {
    const trigger = el('<span class="koyori-mention koyori-mention-gone">@x</span>');
    expect(mentionCardTarget(trigger, table)).toEqual({ kind: 'gone' });
  });

  it('いま入れる者・外れた者は表から引く（画像の無い者は avatarUrl が null）', () => {
    expect(
      mentionCardTarget(el(`<a class="koyori-mention" data-user-id="${A}">@alice</a>`), table),
    ).toEqual({
      kind: 'member',
      userId: A,
      name: 'alice',
      avatarUrl: 'https://cdn.example.com/a.png',
      self: false,
    });
    expect(
      mentionCardTarget(
        el(`<span class="koyori-mention" data-user-id="${B}">@former</span>`),
        table,
      ),
    ).toEqual({ kind: 'outside', userId: B, name: 'former', avatarUrl: null, self: false });
  });

  it('本人の札は self（大文字の currentUserId・大文字の data-user-id でも引ける）', () => {
    const trigger = el(`<a class="koyori-mention" data-user-id="${L.toUpperCase()}">@letters</a>`);
    expect(mentionCardTarget(trigger, table, L.toUpperCase())).toMatchObject({
      kind: 'member',
      userId: L,
      self: true,
    });
    expect(mentionCardTarget(trigger, table, B)).toMatchObject({ self: false });
  });

  it('外れた者は currentUserId と同じ id でも self にならない', () => {
    const trigger = el(`<span class="koyori-mention" data-user-id="${B}">@former</span>`);
    expect(mentionCardTarget(trigger, table, B)).toMatchObject({ kind: 'outside', self: false });
  });

  it('表に無い・status が食い違う・名が無い・id が壊れた札は null', () => {
    const open = (id: string) => el(`<a class="koyori-mention" data-user-id="${id}">@x</a>`);
    expect(mentionCardTarget(open(MISSING), table)).toBeNull();
    expect(mentionCardTarget(open(D), table)).toBeNull(); // 表では gone
    expect(mentionCardTarget(open(C), table)).toBeNull(); // 名が無い
    expect(mentionCardTarget(open(E), table)).toBeNull();
    expect(mentionCardTarget(open('not-a-uuid'), table)).toBeNull();
    expect(mentionCardTarget(el('<a class="koyori-mention">@x</a>'), table)).toBeNull();
  });
});

describe('profileMetaLine', () => {
  it('役職・部署・チームの順に、値のある項目だけを「 · 」でつなぐ', () => {
    expect(
      profileMetaLine({ position: 'デザイナー', department: 'プロダクト部', team: 'UI チーム' }),
    ).toBe('デザイナー · プロダクト部 · UI チーム');
    expect(profileMetaLine({ position: 'エンジニア', department: '開発部', team: null })).toBe(
      'エンジニア · 開発部',
    );
    expect(profileMetaLine({ team: 'UI チーム' })).toBe('UI チーム');
  });

  it('空白だけの項目は省き、全部無ければ null', () => {
    expect(profileMetaLine({ position: '  ', department: ' 開発部 ', team: '' })).toBe('開発部');
    expect(profileMetaLine({ position: ' ', department: '', team: null })).toBeNull();
    expect(profileMetaLine({})).toBeNull();
  });
});

describe('hasMentionCards', () => {
  it('札のある HTML だけ true', () => {
    expect(
      hasMentionCards('<p><span class="koyori-mention koyori-mention-gone">@x</span></p>'),
    ).toBe(true);
    expect(hasMentionCards('<p>ただの本文</p>')).toBe(false);
  });
});

describe('札の HTML とカードの判定の一致', () => {
  it('セレクターに合う札の集合と、対象が引ける札の集合が等しい（空のカードが開かない）', async () => {
    // A は知らない status（unknown の札）、MISSING は表に無い
    const mixed: MentionTable = { ...table, [A]: { status: 'bot' as never, name: 'x' } };
    const ids = [A, B, C, D, E, L, MISSING];
    for (const mentionTenant of [undefined, 'acme']) {
      const html = await renderDescription(ids.map((id) => `<@${id}>`).join(' '), {
        mentions: mixed,
        currentUserId: B,
        mentionTenant,
      });
      const root = document.createElement('div');
      root.innerHTML = html;
      const chips = [...root.querySelectorAll('.koyori-mention')];
      expect(chips).toHaveLength(ids.length);
      const bySelector = chips.filter((c) => c.matches(MENTION_CARD_SELECTOR));
      const byTarget = chips.filter((c) => mentionCardTarget(c, mixed, B) !== null);
      expect(bySelector).toHaveLength(3); // B（外れた者）・D（退会）・L（メンバー）
      expect(bySelector).toEqual(byTarget);
    }
  });

  it('member・名のある outside・gone が開き、名の無い outside・unknown は開かない', async () => {
    const html = await renderDescription(`<@${A}> <@${B}> <@${C}> <@${D}> <@${MISSING}>`, {
      mentions: table,
      mentionTenant: 'acme',
    });
    const root = document.createElement('div');
    root.innerHTML = html;
    const opens = [...root.querySelectorAll('.koyori-mention')].map((c) =>
      c.matches(MENTION_CARD_SELECTOR),
    );
    expect(opens).toEqual([true, true, false, true, false]);
  });
});
