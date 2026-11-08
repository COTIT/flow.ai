-- Developer gap ledger: voice requests the assistant could not finish. Redacted, 90-day retention.
CREATE TABLE assistant_gaps (
 id uuid PRIMARY KEY DEFAULT uuidv7(),
 workspace_id uuid NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
 domain text NOT NULL CHECK(domain ~ '^[a-z_]{1,40}$'),
 language text NOT NULL CHECK(language IN ('en','nl','sw')),
 intent text NOT NULL CHECK(intent ~ '^[a-z]+( [a-z]+)*$' AND length(intent)<=120),
 tools text[] NOT NULL DEFAULT '{}' CHECK(cardinality(tools)<=10),
 outcome text NOT NULL CHECK(outcome IN ('unsupported','refused','failed','cancelled','dropped')),
 http_status smallint CHECK(http_status BETWEEN 400 AND 599),
 follow_up text CHECK(follow_up IN ('rephrased','used_ui')),
 follow_up_route text CHECK(follow_up_route ~ '^(/([a-z]+(-[a-z]+)*|\[id\]))+$' AND length(follow_up_route)<=80),
 created_at timestamptz NOT NULL DEFAULT now(),
 CHECK((follow_up='used_ui')=(follow_up_route IS NOT NULL))
);
CREATE INDEX assistant_gaps_created_idx ON assistant_gaps(created_at);
CREATE INDEX assistant_gaps_workspace_idx ON assistant_gaps(workspace_id,created_at);
