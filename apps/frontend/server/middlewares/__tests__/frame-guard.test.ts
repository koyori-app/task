// @vitest-environment node
import { apply } from '@vikejs/elysia';
import { Elysia } from 'elysia';
import { describe, expect, it } from 'vitest';

import { frameGuard } from '../frame-guard';

// ミドルウェア単体を呼ぶ形にしない。`vike(app, middlewares)` と同じ `apply` で Elysia に載せ、
// 後段のハンドラー（ページの代わり）が返した HTML の応答にヘッダーが付くことを見る。
function appWithPage() {
  const app = new Elysia();
  apply(app, [
    frameGuard,
    () =>
      new Response('<!doctype html><title>page</title>', {
        headers: { 'content-type': 'text/html' },
      }),
  ]);
  return app;
}

async function get(pathname: string) {
  return appWithPage().handle(new Request(`https://task.example.com${pathname}`));
}

describe('frameGuard', () => {
  it.each([
    '/oauth/authorize',
    '/oauth/authorize/',
    '/oauth/authorize//',
    '/OAuth/Authorize',
    '/oauth/authorize?resume=x',
    '/oauth/authorize?client_id=https%3A%2F%2Fclaude.ai%2Foauth%2Fmcp-oauth-client-metadata',
  ])('forbids framing %s', async (pathname) => {
    const response = await get(pathname);

    expect(response.status).toBe(200);
    expect(await response.text()).toContain('<title>page</title>');
    expect(response.headers.get('content-security-policy')).toBe("frame-ancestors 'none'");
    expect(response.headers.get('x-frame-options')).toBe('DENY');
  });

  it.each([
    '/signin',
    '/',
    '/oauth',
    '/oauth/authorizex',
    '/oauth/authorize/extra',
    '/api/oauth/authorize',
    '/desktop/authorize',
  ])('leaves %s alone', async (pathname) => {
    const response = await get(pathname);

    expect(response.status).toBe(200);
    expect(response.headers.get('content-security-policy')).toBeNull();
    expect(response.headers.get('x-frame-options')).toBeNull();
  });
});
