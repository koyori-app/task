import { enhance, type UniversalHandler } from "@universal-middleware/core";

// Note: You can directly define a server middleware instead of defining a Universal Middleware. (You can remove @universal-middleware/* — Vike's scaffolder uses it only to simplify its internal logic, see https://github.com/vikejs/vike/discussions/3116)
export const createTodoHandler: UniversalHandler<Universal.Context & object> = enhance(
  async (request, _context, _runtime) => {
    // In a real case, user-provided data should ALWAYS be validated with tools like zod
    const newTodo = (await request.json()) as { text: string };

    // This is where you'd persist the data
    console.log("Received new todo", newTodo);

    return new Response(JSON.stringify({ status: "OK" }), {
      status: 200,
      headers: {
        "content-type": "application/json",
      },
    });
  },
  { name: "my-app:todo-handler", path: `/api/todo/create`, method: ["GET", "POST"], immutable: false },
);
