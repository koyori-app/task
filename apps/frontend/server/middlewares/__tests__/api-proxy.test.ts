// @vitest-environment node
import { Elysia } from 'elysia';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { MAX_PROXY_BODY_BYTES, apiProxyPlugin } from '../api-proxy';

const app = new Elysia().use(apiProxyPlugin);

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

  it('returns 413 and cancels the source stream when chunked body exceeds MAX_PROXY_BODY_BYTES', async () => {
    let readerCancelCalled = false;
    const totalBytes = MAX_PROXY_BODY_BYTES + 1;
    const chunkSize = 64 * 1024;
    let sent = 0;

    const trackedBody = new ReadableStream({
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

    const originalGetReader = ReadableStream.prototype.getReader.bind(ReadableStream.prototype);
    vi.spyOn(trackedBody, 'getReader').mockImplementation(
      function (this: ReadableStream<Uint8Array>) {
        const reader = originalGetReader.call(this);
        const originalCancel = reader.cancel.bind(reader);
        reader.cancel = async (...args: [] | [reason?: unknown]) => {
          readerCancelCalled = true;
          return originalCancel(...(args as [reason?: unknown]));
        };
        return reader;
      },
    );

    fetchMock.mockImplementation(async (_url, init) => {
      await readStreamBody(init?.body as ReadableStream<Uint8Array> | null);
      return new Response('ok', { status: 200 });
    });

    const response = await proxyRequest(
      new Request('http://localhost/api/v1/upload', {
        method: 'POST',
        headers: { 'content-type': 'application/octet-stream' },
        body: trackedBody,
        // @ts-expect-error Node fetch requires duplex when streaming a request body
        duplex: 'half',
      }),
    );

    expect(response.status).toBe(413);
    expect(await response.text()).toBe('Payload Too Large');
    expect(readerCancelCalled).toBe(true);
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
