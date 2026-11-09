-- Voice gap ledger report, Genesy developers only (run on the server, never exposed to Office).
--
-- All workspaces, redacted (no workspace ids), last 30 days:
--   psql "$DATABASE_URL" -f apps/api/scripts/voice-gaps.sql
-- One workspace's rows:
--   psql "$DATABASE_URL" -v workspace=<uuid> -f apps/api/scripts/voice-gaps.sql
-- Other window: add -v days=7

\if :{?days}
\else
\set days 30
\endif

\if :{?workspace}
SELECT created_at, domain, language, intent, outcome, http_status, tools, follow_up, follow_up_route
FROM assistant_gaps
WHERE workspace_id = :'workspace' AND created_at > now() - make_interval(days => :days)
ORDER BY created_at DESC
LIMIT 500;
\else
-- What to build next: biggest clusters first. `workspaces` = how many accounts hit it.
SELECT domain, outcome, intent,
       count(*) AS gaps,
       count(DISTINCT workspace_id) AS workspaces,
       count(*) FILTER (WHERE follow_up = 'rephrased') AS rephrased,
       count(*) FILTER (WHERE follow_up = 'used_ui') AS used_ui,
       mode() WITHIN GROUP (ORDER BY follow_up_route) AS usual_route,
       string_agg(DISTINCT language, ',') AS languages
FROM assistant_gaps
WHERE created_at > now() - make_interval(days => :days)
GROUP BY domain, outcome, intent
ORDER BY gaps DESC, workspaces DESC
LIMIT 100;
\endif
