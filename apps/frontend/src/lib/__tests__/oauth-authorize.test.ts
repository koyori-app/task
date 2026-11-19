import { beforeEach, describe, expect, it } from 'vitest';
import { signInUrl } from '@/composables/useRedirectAfter';
import {
  authorizeErrorMessage,
  parseAuthorizeQuery,
  readAuthorizeQuery,
  removeAuthorizeQuery,
  resumeUrl,
  stashAuthorizeQuery,
} from '../oauth-authorize';

const QUERY =
  'response_type=code&client_id=https%3A%2F%2Fclaude.ai%2Foauth%2Fmcp-oauth-client-metadata' +
  '&redirect_uri=https%3A%2F%2Fclaude.ai%2Fapi%2Fmcp%2Fauth_callback' +
  '&code_challenge=E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM&code_challenge_method=S256' +
  '&state=st-1&scope=read%3Atask+write%3Atask&resource=https%3A%2F%2Ftask.example.com%2Fapi%2Fmcp';

beforeEach(() => sessionStorage.clear());

describe('parseAuthorizeQuery', () => {
  it('クライアントが付けてきた query を、同意の API の要求の形にする', () => {
    expect(parseAuthorizeQuery(new URLSearchParams(QUERY))).toEqual({
      client_id: 'https://claude.ai/oauth/mcp-oauth-client-metadata',
      redirect_uri: 'https://claude.ai/api/mcp/auth_callback',
      response_type: 'code',
      code_challenge: 'E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM',
      code_challenge_method: 'S256',
      state: 'st-1',
      scope: 'read:task write:task',
      resource: 'https://task.example.com/api/mcp',
    });
  });

  it.each(['client_id', 'redirect_uri'])('%s が無ければ null', (missing) => {
    const search = new URLSearchParams(QUERY);
    search.delete(missing);
    expect(parseAuthorizeQuery(search)).toBeNull();
  });

  it('client_id が空でも null', () => {
    const search = new URLSearchParams(QUERY);
    search.set('client_id', '');
    expect(parseAuthorizeQuery(search)).toBeNull();
  });

  it('ほかの項目は欠けていても読む（API が理由を付けて断る）', () => {
    const parsed = parseAuthorizeQuery(
      new URLSearchParams(
        'client_id=https%3A%2F%2Fc.test%2Fm.json&redirect_uri=https%3A%2F%2Fc.test%2Fcb',
      ),
    );
    expect(parsed).toEqual({
      client_id: 'https://c.test/m.json',
      redirect_uri: 'https://c.test/cb',
      response_type: '',
      code_challenge: '',
      code_challenge_method: '',
      state: null,
      scope: null,
      resource: null,
    });
  });
});

describe('認可の要求の保存', () => {
  it('保存した query が元のまま戻り、何度読んでも同じ値が返る', () => {
    const key = stashAuthorizeQuery(QUERY);

    expect(readAuthorizeQuery(key)).toBe(QUERY);
    expect(readAuthorizeQuery(key)).toBe(QUERY);
  });

  it('消したあとは null', () => {
    const key = stashAuthorizeQuery(QUERY);
    removeAuthorizeQuery(key);
    expect(readAuthorizeQuery(key)).toBeNull();
  });

  it('別のキーの中身には触れない', () => {
    const first = stashAuthorizeQuery(QUERY);
    const second = stashAuthorizeQuery('client_id=other');
    expect(first).not.toBe(second);
    removeAuthorizeQuery(second);
    expect(readAuthorizeQuery(first)).toBe(QUERY);
  });

  it('キーは戻り先の検査に捨てられない文字だけでできている', () => {
    for (let i = 0; i < 50; i += 1) {
      expect(stashAuthorizeQuery(QUERY)).toMatch(/^[A-Za-z0-9_-]{22}$/);
    }
  });

  it.each(['a:b', 'a..b', 'a@b', 'a/b', 'a b', '', '../x'])('キー %j は読まない', (key) => {
    sessionStorage.setItem(`oauth-authorize:${key}`, QUERY);
    expect(readAuthorizeQuery(key)).toBeNull();
  });

  it('保存したことのないキーは null', () => {
    expect(readAuthorizeQuery('never-stashed')).toBeNull();
  });
});

describe('サインインの戻り先', () => {
  it('保存のキーを付けた URL は、戻り先の検査で `/` に潰れない', () => {
    const key = stashAuthorizeQuery(QUERY);
    const target = resumeUrl(key);

    expect(target).toBe(`/oauth/authorize?resume=${key}`);
    expect(signInUrl(target)).toBe(`/signin?redirect_after=${encodeURIComponent(target)}`);
  });

  // クライアントが `:` をエンコードして送るとは限らない
  it.each([
    '/oauth/authorize?scope=read:task',
    '/oauth/authorize?redirect_uri=https://claude.ai/api/mcp/auth_callback',
    '/oauth/authorize?client_id=https%3A%2F%2Fclaude.ai%2Fx&login_hint=a@b.test',
  ])('対照: 生の query を付けた URL（%s）は `/` に潰れる', (target) => {
    expect(signInUrl(target)).toBe('/signin');
  });
});

describe('authorizeErrorMessage', () => {
  it.each([
    ['oauth-client-not-allowed', 'このサーバーでは許可されていないクライアントです。'],
    ['oauth-invalid-redirect-uri', '戻り先の URL が、クライアントの登録と一致しません。'],
    ['oauth-invalid-scope', 'クライアントが求めている権限は、このサーバーでは渡せません。'],
  ])('%s を日本語の文にする', (message, text) => {
    expect(authorizeErrorMessage(message)).toBe(text);
  });

  it.each([
    'oauth-invalid-client',
    'oauth-client-metadata-unavailable',
    'oauth-unsupported-response-type',
    'oauth-invalid-code-challenge',
    'oauth-invalid-target',
  ])('%s に専用の文がある', (message) => {
    expect(authorizeErrorMessage(message)).not.toBe(authorizeErrorMessage('unknown'));
  });

  it('知らない値と文字列でない値は、やり直しの案内にする', () => {
    const fallback = '接続の要求を確かめられませんでした。クライアントからやり直してください。';
    expect(authorizeErrorMessage('bad-request')).toBe(fallback);
    expect(authorizeErrorMessage(undefined)).toBe(fallback);
    expect(authorizeErrorMessage({ message: 'x' })).toBe(fallback);
    // Object.prototype の名前に引っ掛からない
    expect(authorizeErrorMessage('toString')).toBe(fallback);
  });
});
