import { createTodoHandler } from "#/middlewares/create-todo-handler";
import { settingInjector } from "#/middlewares/setting-injector";
import { staticPlugin } from '@elysiajs/static';
import vike from '@vikejs/elysia';
import { Elysia } from 'elysia';

function getApp() {
  const app = new Elysia();

  app.use(staticPlugin({ assets: 'server/public', prefix: '/static-assets' }));

  vike(app, [settingInjector, createTodoHandler]);

  return app;
}

export const app = getApp();
