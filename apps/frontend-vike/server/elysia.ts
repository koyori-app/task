import { createTodoHandler } from "./create-todo-handler";
import { settingInjector } from "./middlewares/setting-injector";
import vike from '@vikejs/elysia';
import { Elysia } from 'elysia';

function getApp() {
  const app = new Elysia();

  vike(app, [settingInjector, createTodoHandler]);

  return app;
}

export const app = getApp();
