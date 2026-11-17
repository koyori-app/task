import { describe, expect, it } from 'vitest';
import {
  isReauthFresh,
  parseReauthMarker,
  REAUTH_MARGIN_MS,
  reauthOAuthLabel,
  reauthOperationLabel,
  reauthWindowLabel,
  type ReauthStatus,
} from '../reauth';

const NOW = Date.parse('2026-11-18T00:00:00Z');

function status(fresh: boolean, expiresInMs: number | null): ReauthStatus {
  return {
    fresh,
    expires_at: expiresInMs === null ? null : new Date(NOW + expiresInMs).toISOString(),
    window_secs: 300,
    methods: { password: true, passkey: false, oauth: [] },
  };
}

describe('isReauthFresh', () => {
  it('印が有効期間の内で、切れるまで余裕があれば撃てる', () => {
    expect(isReauthFresh(status(true, 120_000), NOW)).toBe(true);
    expect(isReauthFresh(status(true, REAUTH_MARGIN_MS + 1), NOW)).toBe(true);
  });

  it('切れる間際の印は無いものとみなす（問うてから撃つまでに切れると 403 の頁で止まる）', () => {
    expect(isReauthFresh(status(true, REAUTH_MARGIN_MS), NOW)).toBe(false);
    expect(isReauthFresh(status(true, 1_000), NOW)).toBe(false);
  });

  it('印が無ければ撃てない', () => {
    expect(isReauthFresh(status(false, null), NOW)).toBe(false);
    // fresh が false なら時刻があっても信じない
    expect(isReauthFresh(status(false, 120_000), NOW)).toBe(false);
    expect(isReauthFresh(status(true, null), NOW)).toBe(false);
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
