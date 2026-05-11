import { createTodoHandler } from "./create-todo-handler";
import vike from "@vikejs/hono";
import { Hono } from "hono";

function getApp() {
  const app = new Hono();

  vike(app, [createTodoHandler]);

  return app;
}

export const app = getApp();
