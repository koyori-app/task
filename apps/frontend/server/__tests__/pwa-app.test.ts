// PWA の配信のヘッダーと manifest（サーバーのアプリ全体を通した確認）
import { describe, expect, it, vi } from 'vitest';

// server/settings/env.ts validates APP_URL when the module loads.
vi.hoisted(() => {
  process.env.APP_URL ??= 'http://localhost:3000';
});

import { getApp } from '../elysia';

// The real Elysia app, as +server.ts builds it: the manifest route must be registered
// after Vike and keep its own content type (the static file server would say octet-stream).
describe('getApp: /manifest.webmanifest', () => {
  it('is served as application/manifest+json', async () => {
    const route = getApp().routes.find(
      ({ method, path }) => method === 'GET' && path === '/manifest.webmanifest',
    );
    expect(route).toBeDefined();

    // Vike 0.4's global middleware needs a built or dev server context. Invoke the
    // registered application route directly so this unit test stays scoped to PWA delivery.
    const res = await route!.handler({} as never);
    if (!(res instanceof Response)) {
      throw new TypeError('The manifest route must return a Response');
    }

    expect(res).toBeInstanceOf(Response);
    expect(res.status).toBe(200);
    expect(res.headers.get('Content-Type')).toMatch(/^application\/manifest\+json/);
  });
});
