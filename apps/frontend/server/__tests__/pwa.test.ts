import { Elysia } from 'elysia';
import { describe, expect, it } from 'vitest';

import { webAppManifest } from '@/lib/pwa/app-manifest';
import { pwaManifestPlugin, pwaStaticHeaders } from '../pwa';

describe('pwaManifestPlugin', () => {
  const app = new Elysia().use(pwaManifestPlugin);

  it('serves the manifest as application/manifest+json, revalidated every time', async () => {
    const res = await app.handle(new Request('http://localhost/manifest.webmanifest'));

    expect(res.status).toBe(200);
    expect(res.headers.get('Content-Type')).toMatch(/^application\/manifest\+json/);
    expect(res.headers.get('Cache-Control')).toBe('no-cache');
    expect(await res.json()).toEqual(webAppManifest);
  });

  it('keeps id, start_url and scope at the origin root', async () => {
    const res = await app.handle(new Request('http://localhost/manifest.webmanifest'));
    const body = (await res.json()) as typeof webAppManifest;

    // Changing id makes installed copies a different app; see app-manifest.ts.
    expect([body.id, body.start_url, body.scope]).toEqual(['/', '/', '/']);
    expect(body.display).toBe('standalone');
    expect(body.icons.map((i) => i.sizes)).toEqual(['192x192', '512x512', '512x512']);
  });
});

describe('pwaStaticHeaders', () => {
  const swResponse = () =>
    new Response('self.addEventListener("install", () => {});', {
      headers: { 'Content-Type': 'text/javascript', ETag: 'W/"1"' },
    });

  it('serves /sw.js with no-cache and a JavaScript content type', async () => {
    const res = await pwaStaticHeaders(new Request('http://localhost/sw.js'), swResponse);

    expect(res.status).toBe(200);
    expect(res.headers.get('Cache-Control')).toBe('no-cache');
    expect(res.headers.get('Content-Type')).toBe('text/javascript; charset=utf-8');
    expect(res.headers.get('ETag')).toBe('W/"1"');
    expect(await res.text()).toContain('install');
  });

  it('keeps no-cache on a 304 for /sw.js', async () => {
    const res = await pwaStaticHeaders(
      new Request('http://localhost/sw.js'),
      () => new Response(null, { status: 304, headers: { ETag: 'W/"1"' } }),
    );

    expect(res.status).toBe(304);
    expect(res.headers.get('Cache-Control')).toBe('no-cache');
  });

  it('does not dress up an HTML fallback for /sw.js as JavaScript', async () => {
    const html = new Response('<!doctype html>', {
      headers: { 'Content-Type': 'text/html;charset=utf-8', 'Cache-Control': 'no-store' },
    });
    const res = await pwaStaticHeaders(new Request('http://localhost/sw.js'), () => html);

    expect(res).toBe(html);
  });

  it('leaves other paths alone', async () => {
    const asset = new Response('x', { headers: { 'Content-Type': 'text/javascript' } });
    const res = await pwaStaticHeaders(
      new Request('http://localhost/assets/entries/a.js'),
      () => asset,
    );

    expect(res).toBe(asset);
  });
});
