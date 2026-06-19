import { Elysia } from 'elysia';

const API_BASE = process.env.API_BASE ?? 'http://localhost:3400';

const HOP_BY_HOP = new Set([
  'connection',
  'keep-alive',
  'proxy-authenticate',
  'proxy-authorization',
  'te',
  'trailers',
  'transfer-encoding',
  'upgrade',
  'host',
]);

function buildBackendUrl(request: Request): string {
  const url = new URL(request.url);
  const backendPath = url.pathname.replace(/^\/api/, '') + url.search;
  return `${API_BASE}${backendPath}`;
}

function copyHeaders(source: Headers, skipHopByHop = true): Headers {
  const headers = new Headers();
  source.forEach((value, key) => {
    if (skipHopByHop && HOP_BY_HOP.has(key.toLowerCase())) return;
    headers.set(key, value);
  });
  return headers;
}

async function proxyToBackend(request: Request): Promise<Response> {
  const backendUrl = buildBackendUrl(request);
  const hasBody = request.method !== 'GET' && request.method !== 'HEAD';

  const backendResponse = await fetch(backendUrl, {
    method: request.method,
    headers: copyHeaders(request.headers),
    body: hasBody ? request.body : undefined,
    // @ts-expect-error Node fetch requires duplex when streaming a request body
    duplex: hasBody ? 'half' : undefined,
  });

  return new Response(backendResponse.body, {
    status: backendResponse.status,
    statusText: backendResponse.statusText,
    headers: copyHeaders(backendResponse.headers),
  });
}

/**
 * Dev/prod SSR: forward /api/v1/* to the Rust backend with /api stripped.
 * Local SSR routes (/api/todo/create, /internal/*) stay on Elysia.
 */
export const apiProxyPlugin = new Elysia({ name: 'api-proxy' }).all('/api/v1/*', ({ request }) =>
  proxyToBackend(request),
);
