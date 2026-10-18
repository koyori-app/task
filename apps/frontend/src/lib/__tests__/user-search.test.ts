import { describe, expect, it } from 'vitest';

import { normalizeUserQuery, orderUsersForPicker, rankUsersByQuery } from '@/lib/user-search';

const users = [
  { id: 'u1', username: 'sousuke' },
  { id: 'u2', username: 'Yupix' },
  { id: 'u3', username: 'kousuke' },
  { id: 'u4', username: 'ユウキ' },
  { id: 'u5', username: 'alice' },
];

describe('normalizeUserQuery', () => {
  it('全角を半角に、大文字を小文字に、片仮名を平仮名に揃える', () => {
    expect(normalizeUserQuery('ＹＵＰＩＸ')).toBe('yupix');
    expect(normalizeUserQuery('ユウキ')).toBe('ゆうき');
    expect(normalizeUserQuery('  Sou ')).toBe('sou');
  });
});

describe('rankUsersByQuery', () => {
  it('前方一致を先に、次に部分一致を返す', () => {
    expect(rankUsersByQuery(users, 'su').map((u) => u.username)).toEqual(['sousuke', 'kousuke']);
    expect(rankUsersByQuery(users, 'suke').map((u) => u.username)).toEqual(['sousuke', 'kousuke']);
    expect(rankUsersByQuery(users, 'kou').map((u) => u.username)).toEqual(['kousuke']);
  });

  it('大文字・全角・片仮名と平仮名の違いを問わない', () => {
    expect(rankUsersByQuery(users, 'yu').map((u) => u.id)).toEqual(['u2']);
    expect(rankUsersByQuery(users, 'ＹＵ').map((u) => u.id)).toEqual(['u2']);
    expect(rankUsersByQuery(users, 'ゆう').map((u) => u.id)).toEqual(['u4']);
  });

  it('空の入力は全員を渡された順で返す', () => {
    expect(rankUsersByQuery(users, '  ')).toEqual(users);
  });
});

describe('orderUsersForPicker', () => {
  it('選んだ者、自分、その他の順。各組の中は名の順', () => {
    const ordered = orderUsersForPicker(users, ['u2', 'u1'], 'u5');

    expect(ordered.map((u) => u.username)).toEqual([
      'sousuke',
      'Yupix',
      'alice',
      'kousuke',
      'ユウキ',
    ]);
  });

  it('自分が選ばれておれば選んだ者の組に入る', () => {
    const ordered = orderUsersForPicker(users, ['u5'], 'u5');

    expect(ordered[0].id).toBe('u5');
  });
});
