// @vitest-environment node
import { Elysia } from 'elysia';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { MAX_PROXY_BODY_BYTES, apiProxyPlugin, limitReadableStream } from '../api-proxy';

const app = new Elysia().use(apiProxyPlugin);
const DEFAULT_MAX_PROXY_BODY_BYTES = 100 * 1024 * 1024;

async function proxyRequest(request: Request): Promise<Response> {
  return app.handle(request);
}

async function readStreamBody(body: ReadableStream<Uint8Array> | null): Promise<Uint8Array> {
  if (!body) return new Uint8Array();

  const reader = body.getReader();
  const chunks: Uint8Array[] = [];
  let total = 0;

  while (true) {
    const { done, value } = await reader.read();
    if (done) break;
    chunks.push(value);
    total += value.byteLength;
  }

  const merged = new Uint8Array(total);
  let offset = 0;
  for (const chunk of chunks) {
    merged.set(chunk, offset);
    offset += chunk.byteLength;
  }

  return merged;
}

describe('limitReadableStream', () => {
  it('cancels the source reader when streamed bytes exceed the configured max', async () => {
    let sourceCancelCalled = false;
    const source = new ReadableStream<Uint8Array>({
      pull(controller) {
        controller.enqueue(new Uint8Array(64));
      },
      cancel() {
        sourceCancelCalled = true;
      },
    });

    const limited = limitReadableStream(source, 96);
    const reader = limited.stream.getReader();

    await expect(reader.read()).resolves.toEqual({
      done: false,
      value: new Uint8Array(64),
    });
    await expect(reader.read()).rejects.toThrow('Payload Too Large');
    expect(sourceCancelCalled).toBe(true);
  });
});

describe('MAX_PROXY_BODY_BYTES', () => {
  afterEach(() => {
    vi.unstubAllEnvs();
    vi.resetModules();
  });

  it('parses a positive UPLOAD_MAX_SIZE_MB value', async () => {
    vi.stubEnv('UPLOAD_MAX_SIZE_MB', '2.5');
    vi.resetModules();

    const proxy = await import('../api-proxy');

    expect(proxy.MAX_PROXY_BODY_BYTES).toBe(2.5 * 1024 * 1024);
  });

  it('defaults to 100 MiB when UPLOAD_MAX_SIZE_MB is unset', async () => {
    vi.stubEnv('UPLOAD_MAX_SIZE_MB', undefined);
    vi.resetModules();

    const proxy = await import('../api-proxy');

    expect(proxy.MAX_PROXY_BODY_BYTES).toBe(DEFAULT_MAX_PROXY_BODY_BYTES);
  });

  it.each(['abc', 'NaN', '0', '-1'])('fails fast when UPLOAD_MAX_SIZE_MB is %j', async (value) => {
    vi.stubEnv('UPLOAD_MAX_SIZE_MB', value);
    vi.resetModules();

    await expect(import('../api-proxy')).rejects.toThrow(/UPLOAD_MAX_SIZE_MB/);
  });
});

describe('apiProxyPlugin', () => {
  const fetchMock = vi.fn<typeof fetch>();

  beforeEach(() => {
    fetchMock.mockReset();
    vi.stubGlobal('fetch', fetchMock);
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('returns 413 before backend fetch when Content-Length exceeds MAX_PROXY_BODY_BYTES', async () => {
    const response = await proxyRequest(
      new Request('http://localhost/api/v1/upload', {
        method: 'POST',
        headers: {
          'content-type': 'application/octet-stream',
          'content-length': String(MAX_PROXY_BODY_BYTES + 1),
        },
        body: 'too-large',
      }),
    );

    expect(response.status).toBe(413);
    expect(await response.text()).toBe('Payload Too Large');
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it('returns 413 when chunked body exceeds MAX_PROXY_BODY_BYTES without Content-Length', async () => {
    const totalBytes = MAX_PROXY_BODY_BYTES + 1;
    const chunkSize = 64 * 1024;
    let sent = 0;

    const oversizedBody = new ReadableStream({
      pull(controller) {
        if (sent >= totalBytes) {
          controller.close();
          return;
        }

        const size = Math.min(chunkSize, totalBytes - sent);
        controller.enqueue(new Uint8Array(size));
        sent += size;
      },
    });

    fetchMock.mockImplementation(async (_url, init) => {
      try {
        await readStreamBody(init?.body as ReadableStream<Uint8Array> | null);
      } catch (cause) {
        // undici wraps stream errors as TypeError('fetch failed', { cause })
        throw new TypeError('fetch failed', { cause });
      }
      return new Response('ok', { status: 200 });
    });

    const response = await proxyRequest(
      new Request('http://localhost/api/v1/upload', {
        method: 'POST',
        headers: { 'content-type': 'application/octet-stream' },
        body: oversizedBody,
        // @ts-expect-error Node fetch requires duplex when streaming a request body
        duplex: 'half',
      }),
    );

    expect(response.status).toBe(413);
    expect(await response.text()).toBe('Payload Too Large');
  });

  it('forwards an under-limit streamed body to the backend unchanged', async () => {
    const payload = new TextEncoder().encode('hello-proxy-body');
    let forwardedBody: Uint8Array | null = null;

    fetchMock.mockImplementation(async (_url, init) => {
      forwardedBody = await readStreamBody(init?.body as ReadableStream<Uint8Array> | null);
      return new Response('proxied', { status: 201 });
    });

    const response = await proxyRequest(
      new Request('http://localhost/api/v1/items', {
        method: 'POST',
        headers: {
          'content-type': 'text/plain',
          'content-length': String(payload.byteLength),
        },
        body: payload,
      }),
    );

    expect(response.status).toBe(201);
    expect(await response.text()).toBe('proxied');
    expect(forwardedBody).toEqual(payload);
    expect(fetchMock).toHaveBeenCalledOnce();

    const [backendUrl, init] = fetchMock.mock.calls[0]!;
    expect(backendUrl).toBe('http://localhost:3400/v1/items');
    expect(init?.method).toBe('POST');
  });
});
