import { describe, expect, it } from 'vitest';
import {
  isReauthFresh,
  parseReauthMarker,
  REAUTH_MARGIN_SECS,
  reauthOAuthLabel,
  reauthOperationLabel,
  reauthWindowLabel,
  type ReauthStatus,
} from '../reauth';

function status(fresh: boolean, remainingSecs: number | null): ReauthStatus {
  return {
    fresh,
    // 端末の時計とは比べない値。どの時刻でも判定に効かないことを、わざと遠い過去にして見る
    expires_at: remainingSecs === null ? null : '2000-01-01T00:00:00Z',
    remaining_secs: remainingSecs,
    window_secs: 300,
    methods: { password: true, passkey: false, oauth: [] },
  };
}

describe('isReauthFresh', () => {
  it('残りの秒数が余裕を越えていれば撃てる', () => {
    expect(isReauthFresh(status(true, 120))).toBe(true);
    expect(isReauthFresh(status(true, REAUTH_MARGIN_SECS + 1))).toBe(true);
  });

  it('切れる間際の印は無いものとみなす（問うてから撃つまでに切れると 403 の頁で止まる）', () => {
    expect(isReauthFresh(status(true, REAUTH_MARGIN_SECS))).toBe(false);
    expect(isReauthFresh(status(true, 1))).toBe(false);
  });

  it('印が無ければ撃てない', () => {
    expect(isReauthFresh(status(false, null))).toBe(false);
    // fresh が false なら残りがあっても信じない
    expect(isReauthFresh(status(false, 120))).toBe(false);
    expect(isReauthFresh(status(true, null))).toBe(false);
  });
});

describe('parseReauthMarker', () => {
  it('連携の追加と、確かめに使った連携を読む', () => {
    expect(
      parseReauthMarker(
        JSON.stringify({
          kind: 'link',
          provider: 'gitlab_selfhosted',
          instanceUrl: 'https://gitlab.example.com',
          via: 'github',
        }),
      ),
    ).toEqual({
      resume: {
        kind: 'link',
        provider: 'gitlab_selfhosted',
        instanceUrl: 'https://gitlab.example.com',
      },
      via: 'github',
    });
  });

  it('初回のパスワードを読む', () => {
    expect(parseReauthMarker(JSON.stringify({ kind: 'set-password' }))).toEqual({
      resume: { kind: 'set-password' },
      via: null,
    });
  });

  it('知らないプロバイダーや壊れた印は読まない', () => {
    expect(parseReauthMarker(null)).toBeNull();
    expect(parseReauthMarker('{')).toBeNull();
    expect(parseReauthMarker(JSON.stringify({ kind: 'link', provider: 'evil' }))).toBeNull();
    expect(parseReauthMarker(JSON.stringify({ kind: 'delete-everything' }))).toBeNull();
    expect(
      parseReauthMarker(JSON.stringify({ kind: 'set-password', via: 'evil' }))?.via,
    ).toBeNull();
  });
});

describe('dialog の字', () => {
  it('操作の名', () => {
    expect(reauthOperationLabel({ kind: 'set-password' })).toBe('パスワードを設定する');
    expect(reauthOperationLabel({ kind: 'link', provider: 'google' })).toBe(
      'Google の連携を追加する',
    );
  });

  it('有効期間の長さ', () => {
    expect(reauthWindowLabel(300)).toBe('5 分間');
    expect(reauthWindowLabel(90)).toBe('90 秒間');
    expect(reauthWindowLabel(5)).toBe('5 秒間');
  });

  it('self-hosted は instance を添えて見分ける', () => {
    expect(
      reauthOAuthLabel({
        provider: 'gitlab_selfhosted',
        connection_provider: 'gitlab_selfhosted',
        instance_url: 'https://gitlab.example.com',
      }),
    ).toContain('https://gitlab.example.com');
    expect(
      reauthOAuthLabel({ provider: 'github', connection_provider: 'github', instance_url: null }),
    ).toBe('GitHub');
  });
});
