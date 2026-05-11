import { createTodoHandler } from "./create-todo-handler";
import vike from '@vikejs/elysia';
import { Elysia } from 'elysia';

function getApp() {
  const app = new Elysia();

  vike(app, [createTodoHandler]);

  return app;
}

export const app = getApp();
