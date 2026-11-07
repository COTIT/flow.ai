-- psql vars: checkpoint_role (assistant_checkpoint_<environment>), checkpoint_mode.
-- Identities precede restore; bootstrap grants follow SQLx migrations; verify never repairs.
SELECT set_config('genesy.checkpoint_role', :'checkpoint_role', false);
SELECT set_config('genesy.checkpoint_mode', :'checkpoint_mode', false);
DO $$
DECLARE
    runtime_role text := current_setting('genesy.checkpoint_role');
    mode text := current_setting('genesy.checkpoint_mode');
    object_name text;
    privilege_name text;
BEGIN
    IF runtime_role !~ '^assistant_checkpoint_(staging|production|local)$'
       OR mode NOT IN ('identities', 'bootstrap', 'verify') THEN
        RAISE EXCEPTION 'Invalid checkpoint role or mode';
    END IF;
    IF mode <> 'verify' THEN
        IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname = 'assistant_checkpoint_runtime') THEN
            CREATE ROLE assistant_checkpoint_runtime NOLOGIN;
        END IF;
        ALTER ROLE assistant_checkpoint_runtime NOLOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE NOREPLICATION NOBYPASSRLS;
        IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname = runtime_role) THEN
            EXECUTE format('CREATE ROLE %I NOLOGIN', runtime_role);
        END IF;
        -- Two overlapping Office instances: each four shared-pool slots plus one readiness probe.
        EXECUTE format('ALTER ROLE %I NOSUPERUSER NOCREATEDB NOCREATEROLE INHERIT NOREPLICATION NOBYPASSRLS CONNECTION LIMIT 10', runtime_role);
        EXECUTE format('ALTER ROLE %I SET search_path = pg_catalog, assistant_checkpoint', runtime_role);
        EXECUTE format('GRANT assistant_checkpoint_runtime TO %I', runtime_role);
        EXECUTE format('GRANT CONNECT ON DATABASE %I TO %I', current_database(), runtime_role);
        IF mode = 'bootstrap' AND to_regnamespace('assistant_checkpoint') IS NOT NULL THEN
            GRANT USAGE ON SCHEMA assistant_checkpoint TO assistant_checkpoint_runtime;
            GRANT SELECT ON assistant_checkpoint.checkpoint_migrations TO assistant_checkpoint_runtime;
            GRANT SELECT, INSERT, UPDATE, DELETE ON assistant_checkpoint.checkpoints,
                assistant_checkpoint.checkpoint_blobs, assistant_checkpoint.checkpoint_writes TO assistant_checkpoint_runtime;
        END IF;
    ELSE
        IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname = runtime_role)
           OR NOT EXISTS (SELECT FROM pg_roles WHERE rolname = runtime_role AND rolconnlimit = 10)
           OR NOT EXISTS (SELECT FROM pg_roles WHERE rolname = 'assistant_checkpoint_runtime' AND NOT rolcanlogin)
           OR EXISTS (SELECT FROM pg_roles WHERE rolname IN (runtime_role, 'assistant_checkpoint_runtime')
               AND (rolsuper OR rolcreatedb OR rolcreaterole OR rolreplication OR rolbypassrls))
           OR NOT pg_has_role(runtime_role, 'assistant_checkpoint_runtime', 'MEMBER')
           OR EXISTS (SELECT FROM pg_auth_members WHERE member IN
               (SELECT oid FROM pg_roles WHERE rolname IN (runtime_role, 'assistant_checkpoint_runtime'))
               AND (roleid <> 'assistant_checkpoint_runtime'::regrole OR admin_option)) THEN
            RAISE EXCEPTION 'Checkpoint role attributes, memberships or connection limit differ from expected settings';
        END IF;
        IF has_database_privilege(runtime_role, current_database(), 'CREATE')
           OR EXISTS (SELECT FROM pg_namespace WHERE nspname !~ '^pg_temp_'
               AND has_schema_privilege(runtime_role, oid, 'CREATE'))
           OR EXISTS (SELECT FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace
               WHERE n.nspname='public' AND c.relkind IN ('r','p','v','m','S','f')
               AND CASE WHEN c.relkind='S' THEN has_sequence_privilege(runtime_role, c.oid, 'SELECT,UPDATE,USAGE')
                   ELSE has_table_privilege(runtime_role, c.oid, 'SELECT,INSERT,UPDATE,DELETE,TRUNCATE,REFERENCES,TRIGGER')
                       OR has_any_column_privilege(runtime_role, c.oid, 'SELECT,INSERT,UPDATE,REFERENCES') END) THEN
            RAISE EXCEPTION 'Checkpoint role can access business data or create database objects';
        END IF;
        IF NOT has_schema_privilege(runtime_role, 'assistant_checkpoint', 'USAGE')
           OR NOT has_table_privilege(runtime_role, 'assistant_checkpoint.checkpoint_migrations', 'SELECT')
           OR has_table_privilege(runtime_role, 'assistant_checkpoint.checkpoint_migrations', 'INSERT,UPDATE,DELETE,TRUNCATE') THEN
            RAISE EXCEPTION 'Checkpoint schema grants differ from expected grants';
        END IF;
        FOREACH object_name IN ARRAY ARRAY['checkpoints','checkpoint_blobs','checkpoint_writes'] LOOP
            IF has_table_privilege(runtime_role, 'assistant_checkpoint.' || object_name, 'TRUNCATE,REFERENCES,TRIGGER') THEN
                RAISE EXCEPTION 'Unexpected checkpoint privilege: %', object_name;
            END IF;
            FOREACH privilege_name IN ARRAY ARRAY['SELECT','INSERT','UPDATE','DELETE'] LOOP
                IF NOT has_table_privilege(runtime_role, 'assistant_checkpoint.' || object_name, privilege_name) THEN
                    RAISE EXCEPTION 'Missing checkpoint privilege: % %', object_name, privilege_name;
                END IF;
            END LOOP;
        END LOOP;
        IF (SELECT max(v) FROM assistant_checkpoint.checkpoint_migrations) IS DISTINCT FROM 5 THEN
            RAISE EXCEPTION 'Expected PostgresSaver 1.1.2 schema v5';
        END IF;
    END IF;
END $$;
