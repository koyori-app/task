import { enhance, MiddlewareOrder, type UniversalMiddleware } from '@universal-middleware/core';

const GUARDED_PATHS = new Set(['/signin', '/signup']);
const SENSITIVE_QUERY_KEYS = ['password', 'email'];

export const prehydrationQueryGuard: UniversalMiddleware = enhance(
  async (request, context) => {
    if (request.method !== 'GET') {
      return context;
    }

    const url = new URL(request.url);
    if (!GUARDED_PATHS.has(url.pathname)) {
      return context;
    }

    let changed = false;
    for (const key of SENSITIVE_QUERY_KEYS) {
      if (url.searchParams.has(key)) {
        url.searchParams.delete(key);
        changed = true;
      }
    }

    if (!changed) {
      return context;
    }

    return new Response(null, {
      status: 302,
      headers: {
        Location: `${url.pathname}${url.search}${url.hash}`,
      },
    });
  },
  {
    name: 'app:prehydration-query-guard',
    order: MiddlewareOrder.CUSTOM_PRE_PROCESSING,
    immutable: true,
  },
);
