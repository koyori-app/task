import { createTodoHandler } from "#/middlewares/create-todo-handler";
import { settingInjector } from "#/middlewares/setting-injector";
import { staticPlugin } from '@elysiajs/static';
import vike from '@vikejs/elysia';
import { Elysia } from 'elysia';

function getApp() {
  const app = new Elysia();

  // In dev, Vite serves public/ automatically. In prod, serve built client assets.
  if (process.env.NODE_ENV === 'production') {
    app.use(staticPlugin({ assets: 'dist/client', prefix: '/' }));
  }

  vike(app, [settingInjector, createTodoHandler]);

  return app;
}

export const app = getApp();
