import { Elysia } from 'elysia';

const API_BASE = process.env.API_BASE ?? 'http://localhost:3400';

/** Align with backend UPLOAD_MAX_SIZE_MB default (100). */
export const MAX_PROXY_BODY_BYTES = 100 * 1024 * 1024;

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

class BodyTooLargeError extends Error {
  constructor() {
    super('Payload Too Large');
    this.name = 'BodyTooLargeError';
  }
}

function buildBackendUrl(request: Request): string {
  const url = new URL(request.url);
  const backendPath = url.pathname.replace(/^\/api/, '') + url.search;
  return `${API_BASE}${backendPath}`;
}

function copyHeaders(source: Headers, skipHopByHop = true): Headers {
  const headers = new Headers();
  source.forEach((value, key) => {
    const lowerKey = key.toLowerCase();
    if (skipHopByHop && HOP_BY_HOP.has(lowerKey)) return;
    if (lowerKey === 'set-cookie') return;
    headers.set(key, value);
  });
  for (const cookie of source.getSetCookie()) {
    headers.append('Set-Cookie', cookie);
  }
  return headers;
}

function rejectIfContentLengthTooLarge(request: Request): Response | null {
  const contentLength = request.headers.get('content-length');
  if (!contentLength) return null;

  const length = Number(contentLength);
  if (!Number.isFinite(length) || length < 0) return null;
  if (length > MAX_PROXY_BODY_BYTES) {
    return new Response('Payload Too Large', { status: 413 });
  }
  return null;
}

function limitReadableStream(
  body: ReadableStream<Uint8Array>,
  maxBytes: number,
): ReadableStream<Uint8Array> {
  let consumed = 0;
  const reader = body.getReader();

  return new ReadableStream({
    async pull(controller) {
      const { done, value } = await reader.read();
      if (done) {
        controller.close();
        return;
      }

      consumed += value.byteLength;
      if (consumed > maxBytes) {
        await reader.cancel();
        controller.error(new BodyTooLargeError());
        return;
      }

      controller.enqueue(value);
    },
    cancel(reason) {
      return reader.cancel(reason);
    },
  });
}

async function proxyToBackend(request: Request): Promise<Response> {
  const rejected = rejectIfContentLengthTooLarge(request);
  if (rejected) return rejected;

  const backendUrl = buildBackendUrl(request);
  const hasBody = request.method !== 'GET' && request.method !== 'HEAD';

  // parse: 'none' keeps Elysia from consuming the body (b9022093). Stream it through
  // instead of buffering the full payload (bea9a39a workaround).
  const body =
    hasBody && request.body ? limitReadableStream(request.body, MAX_PROXY_BODY_BYTES) : undefined;

  try {
    const backendResponse = await fetch(backendUrl, {
      method: request.method,
      headers: copyHeaders(request.headers),
      body,
      // @ts-expect-error Node/Bun fetch requires duplex when streaming a request body
      duplex: hasBody ? 'half' : undefined,
    });

    return new Response(backendResponse.body, {
      status: backendResponse.status,
      statusText: backendResponse.statusText,
      headers: copyHeaders(backendResponse.headers),
    });
  } catch (error) {
    if (error instanceof BodyTooLargeError) {
      return new Response('Payload Too Large', { status: 413 });
    }
    throw error;
  }
}

/**
 * Dev/prod SSR: forward /api/v1/* to the Rust backend with /api stripped.
 * Local SSR routes (/internal/*) stay on Elysia.
 */
export const apiProxyPlugin = new Elysia({ name: 'api-proxy' }).all(
  '/api/v1/*',
  ({ request }) => proxyToBackend(request),
  { parse: 'none' },
);
