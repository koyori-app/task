import { Elysia } from 'elysia';

import { MANIFEST_CONTENT_TYPE, MANIFEST_PATH, webAppManifest } from '@/lib/pwa/app-manifest';
import { SERVICE_WORKER_PATH } from '@/lib/pwa/register-service-worker';

const manifestBody = JSON.stringify(webAppManifest);

/**
 * /manifest.webmanifest を app-manifest.ts から組んで返す。
 *
 * public/ に置くと、本番の静的な配り（srvx の staticMiddleware）が `.webmanifest` を
 * 知らず application/octet-stream で返す。ここで返せば Content-Type を決められ、
 * 色や icon の置き場も app-manifest.ts の一つで済む。
 */
export const pwaManifestPlugin = new Elysia({ name: 'pwa-manifest' }).get(
  MANIFEST_PATH,
  () =>
    new Response(manifestBody, {
      headers: {
        'Content-Type': MANIFEST_CONTENT_TYPE,
        // 色や icon を差し替えた時にすぐ届くよう、毎回確かめさせる。
        'Cache-Control': 'no-cache',
      },
    }),
);

type Next = () => Response | Promise<Response>;

/**
 * 本番の server（@universal-deploy/node の srvx）の middleware。+server.ts の
 * `prod.middleware` に置く。静的な配りは Elysia より前で走るので、/sw.js の header は
 * Elysia の側では直せない。
 *
 * /sw.js に `Cache-Control: no-cache` を付け、Content-Type を JavaScript に揃える。
 * service worker の更新の確かめが、間の cache（proxy など）で古い版を掴まぬように。
 * 応答が HTML の時（/sw.js が無く SSR が返した時）は触らない。HTML を JavaScript と
 * 偽ると、壊れた service worker を据えることになる。
 */
export async function pwaStaticHeaders(request: Request, next: Next): Promise<Response> {
  const response = await next();
  if (new URL(request.url).pathname !== SERVICE_WORKER_PATH) return response;
  // 304（ETag が合った時）は Content-Type を持たぬが、Cache-Control は揃えて返す。
  const notModified = response.status === 304;
  const type = response.headers.get('Content-Type') ?? '';
  if (!notModified && !type.includes('javascript')) return response;
  const headers = new Headers(response.headers);
  if (!notModified) headers.set('Content-Type', 'text/javascript; charset=utf-8');
  headers.set('Cache-Control', 'no-cache');
  return new Response(response.body, {
    status: response.status,
    statusText: response.statusText,
    headers,
  });
}
