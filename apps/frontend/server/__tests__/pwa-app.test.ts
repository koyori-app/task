import { describe, expect, it, vi } from 'vitest';

// server/settings/env.ts validates APP_URL when the module loads.
vi.hoisted(() => {
  process.env.APP_URL ??= 'http://localhost:3000';
});

import { getApp } from '../elysia';

// The real Elysia app, as +server.ts builds it: the manifest must come out of it
// with its own content type (the static file server would say octet-stream).
describe('getApp: /manifest.webmanifest', () => {
  it('is served as application/manifest+json', async () => {
    const res = await getApp().handle(new Request('http://localhost/manifest.webmanifest'));

    expect(res.status).toBe(200);
    expect(res.headers.get('Content-Type')).toMatch(/^application\/manifest\+json/);
  });
});
