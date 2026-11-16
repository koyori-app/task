INSERT INTO project_task_types (id, project_id, name, color, position, created_at)
SELECT gen_random_uuid(), p.id, d.name, d.color, d.position::smallint, now()
FROM projects p
CROSS JOIN (VALUES ('タスク', '#64748b', 0), ('バグ', '#ef4444', 1), ('機能', '#3b82f6', 2))
    AS d(name, color, position)
WHERE NOT EXISTS (SELECT 1 FROM project_task_types t WHERE t.project_id = p.id)
ON CONFLICT (project_id, name) DO NOTHING;
