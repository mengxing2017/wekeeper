-- Destructive reset for the dedicated disposable development database only.
-- Drop the whole schema so functions, sequences and SQLx history cannot survive.
BEGIN;
DROP SCHEMA IF EXISTS public CASCADE;
CREATE SCHEMA public;
COMMIT;
