-- Component demand: k-anonymous gap clusters across workspaces. No workspace ids, rows or timestamps leave.
-- MIN_WORKSPACES floor = 3 lives only here; callers may raise k, never lower it. last_seen is a UTC day.
CREATE FUNCTION assistant_gap_demand(days int, min_workspaces int DEFAULT 3)
RETURNS TABLE(domain text, outcome text, words text, intent text, gaps bigint, workspaces bigint,
  languages text[], rephrased bigint, used_ui bigint, route text, tool text, http_status smallint,
  last_seen date)
LANGUAGE sql STABLE SECURITY INVOKER AS $$
  WITH g AS (
    SELECT g.*, array_to_string(ARRAY(SELECT DISTINCT w FROM unnest(string_to_array(g.intent,' ')) w ORDER BY w),' ') AS words
    FROM assistant_gaps g
    WHERE g.created_at > now()-make_interval(days => least(greatest(days,1),90))
  )
  SELECT g.domain, g.outcome, g.words,
    mode() WITHIN GROUP (ORDER BY g.intent),
    count(*), count(DISTINCT g.workspace_id),
    array_agg(DISTINCT g.language ORDER BY g.language),
    count(*) FILTER (WHERE g.follow_up='rephrased'),
    count(*) FILTER (WHERE g.follow_up='used_ui'),
    mode() WITHIN GROUP (ORDER BY g.follow_up_route),
    mode() WITHIN GROUP (ORDER BY g.tools[cardinality(g.tools)]),
    mode() WITHIN GROUP (ORDER BY g.http_status),
    (max(g.created_at) AT TIME ZONE 'UTC')::date
  FROM g
  GROUP BY g.domain, g.outcome, g.words
  HAVING count(DISTINCT g.workspace_id) >= greatest(min_workspaces,3)
  ORDER BY 5 DESC, 6 DESC, 1, 2, 3
  LIMIT 200
$$;
