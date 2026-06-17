import { createTodoHandler } from './middlewares/create-todo-handler';
import vike from '@vikejs/hono';
import { Hono } from 'hono';

/**
 * @deprecated 問題がない限りはこのファイルは削除される予定です。elysiaのほうが恐らくbun向きです。
 */
function getApp() {
  const app = new Hono();

  vike(app, [createTodoHandler]);

  return app;
}

export const app = getApp();
