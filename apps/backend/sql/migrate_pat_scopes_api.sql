-- PAT のスコープを api / read_api 方式へ移す（TASK-199。apps/backend/docs/personal-access-tokens-authz.md）。
--
-- admin:tenant  → api（どちらも全スコープを満たす）
-- admin:project → project 層の write:* 6 個（write は対の read を含む）。
--                 api にするとテナント層（read:tenant / write:tenant）まで広がるので、同じ範囲へ展開する
--
-- 何度流しても同じ結果になる（対象の値が残っている行だけを書き換える）。
UPDATE personal_tokens AS t
SET scopes = (
    SELECT COALESCE(jsonb_agg(s ORDER BY s), '[]'::jsonb)
    FROM (
        SELECT DISTINCT s
        FROM (
            SELECT CASE e WHEN 'admin:tenant' THEN 'api' ELSE e END AS s
            FROM jsonb_array_elements_text(t.scopes) AS e
            WHERE e <> 'admin:project'
            UNION ALL
            SELECT x
            FROM unnest(ARRAY[
                'write:project', 'write:drive', 'write:task',
                'write:milestone', 'write:sprint', 'write:review'
            ]) AS x
            WHERE t.scopes @> '["admin:project"]'::jsonb
        ) AS expanded
    ) AS deduplicated
)
WHERE t.scopes @> '["admin:tenant"]'::jsonb
   OR t.scopes @> '["admin:project"]'::jsonb;
