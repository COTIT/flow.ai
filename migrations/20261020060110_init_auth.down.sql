-- Reverse of init_auth. Children first (FKs).
DROP TABLE IF EXISTS access_tokens;
DROP TABLE IF EXISTS invites;
DROP TABLE IF EXISTS workspace_members;
DROP TABLE IF EXISTS workspaces;
DROP TABLE IF EXISTS users;
