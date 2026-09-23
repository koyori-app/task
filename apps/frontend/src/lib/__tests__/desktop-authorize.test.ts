import { describe, expect, it } from 'vitest';
import {
  desktopAuthorizeReturnPath,
  desktopCallbackUrl,
  parseDesktopAuthorizeQuery,
} from '../desktop-authorize';

const CHALLENGE = 'E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM';

function query(overrides: Record<string, string | null> = {}) {
  const params = new URLSearchParams({
    port: '43123',
    code_challenge: CHALLENGE,
    state: 'st',
    name: 'laptop',
  });
  for (const [key, value] of Object.entries(overrides)) {
    if (value === null) params.delete(key);
    else params.set(key, value);
  }
  return params;
}

describe('parseDesktopAuthorizeQuery', () => {
  it('正しいクエリを受け付ける', () => {
    expect(parseDesktopAuthorizeQuery(query())).toEqual({
      port: 43123,
      codeChallenge: CHALLENGE,
      state: 'st',
      name: 'laptop',
    });
  });

  it.each(['1024', '65535'])('port の境界 %s は受け付ける', (port) => {
    expect(parseDesktopAuthorizeQuery(query({ port }))?.port).toBe(Number(port));
  });

  it.each(['1023', '65536', '0', '-1', '8080.5', '0x1f90', ' 8080', '8080a', '', '99999'])(
    'port %j は拒否する',
    (port) => {
      expect(parseDesktopAuthorizeQuery(query({ port }))).toBeNull();
    },
  );

  it('port が無ければ拒否する', () => {
    expect(parseDesktopAuthorizeQuery(query({ port: null }))).toBeNull();
  });

  it.each(['a'.repeat(42), 'a'.repeat(129), `${'a'.repeat(42)}+`])(
    'code_challenge %j は拒否する',
    (codeChallenge) => {
      expect(parseDesktopAuthorizeQuery(query({ code_challenge: codeChallenge }))).toBeNull();
    },
  );

  it('state が無ければ拒否する', () => {
    expect(parseDesktopAuthorizeQuery(query({ state: null }))).toBeNull();
  });

  it('name が無ければ既定名、100 文字を越えれば拒否する', () => {
    expect(parseDesktopAuthorizeQuery(query({ name: null }))?.name).toBe('Koyori Desktop');
    expect(parseDesktopAuthorizeQuery(query({ name: 'n'.repeat(100) }))).not.toBeNull();
    expect(parseDesktopAuthorizeQuery(query({ name: 'n'.repeat(101) }))).toBeNull();
  });
});

describe('desktopCallbackUrl', () => {
  it('loopback の callback に code と state を載せる', () => {
    expect(desktopCallbackUrl(43123, 'c/d+e', 's&t')).toBe(
      'http://127.0.0.1:43123/callback?code=c%2Fd%2Be&state=s%26t',
    );
  });
});

describe('desktopAuthorizeReturnPath', () => {
  it('承認画面のパスだけを戻り先にする', () => {
    expect(desktopAuthorizeReturnPath('/desktop/authorize?port=1')).toBe(
      '/desktop/authorize?port=1',
    );
    expect(desktopAuthorizeReturnPath('https://evil.example/desktop/authorize?')).toBeNull();
    expect(desktopAuthorizeReturnPath('/settings')).toBeNull();
    expect(desktopAuthorizeReturnPath(null)).toBeNull();
  });
});
