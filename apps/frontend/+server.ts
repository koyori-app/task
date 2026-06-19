import type { Server } from 'vike/types';
import { getApp } from './server/elysia';

const port = process.env.PORT ? parseInt(process.env.PORT, 10) : 3000;

let app: ReturnType<typeof getApp> | undefined;

// Lazy-init: Vike HMR can reload +server.ts before elysia exports are ready (app undefined → 500).
function serverApp() {
  app ??= getApp();
  return app;
}

// https://vike.dev/server
export default {
  fetch(request, ...args) {
    return serverApp().fetch(request, ...args);
  },
  prod: {
    port,
  },
} satisfies Server;
