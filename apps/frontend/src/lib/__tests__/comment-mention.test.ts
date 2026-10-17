import { describe, expect, it } from 'vitest';

import {
  filterCandidates,
  findMarkers,
  lookup,
  markerOf,
  mentionLabel,
  mentionTrigger,
  splitBody,
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

describe('splitBody', () => {
  it('地の文と札に分け、既存の素の @username は地の文のまま残す', () => {
    const segments = splitBody(`@legacy さん ${markerOf(A)} へ`, table);
    expect(segments).toEqual([
      { kind: 'text', text: '@legacy さん ' },
      { kind: 'mention', id: A, user: table[A] },
      { kind: 'text', text: ' へ' },
    ]);
  });

  it('表に無い id の user は undefined（不明な札になる）', () => {
    const [segment] = splitBody(markerOf(B.replace('2', '3')), table);
    expect(segment).toMatchObject({ kind: 'mention', user: undefined });
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
