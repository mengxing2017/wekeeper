"""Check runtime isolation migrations and the production retry-budget query in socket-free PG.

Requires --postgres-bin; creates and removes a fresh temporary database, never reads DATABASE_URL.
"""
from pathlib import Path
import argparse
import json
import re
import subprocess
import tempfile

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--postgres-bin', required=True, type=Path)
args = parser.parse_args()
root = Path(__file__).resolve().parents[1]
files = sorted((root / 'backend/migrations').glob('*.sql'))
pre = [p for p in files if p.name < '202611040126']
post = [p for p in files if p not in pre]
seed = """
INSERT INTO official_channel_deliveries(id,corp_id,generation,credential_scope,state,checked_at,valid_until,source)
 VALUES('00000000-0000-4000-8000-000000000001','corp',1,'paused-scope','paused',now()-interval '2 days',now()-interval '1 day','previous explicit pause'),
 ('00000000-0000-4000-8000-000000000002','corp',1,'resumed-scope','paused',now()-interval '2 days',now()-interval '1 day','previous pause'),
 ('00000000-0000-4000-8000-000000000003','corp',1,'resumed-scope','available',now()-interval '1 hour',now()+interval '1 hour','previous explicit resume');
"""
checks = """
DO $$ BEGIN
 IF to_regclass('official_channel_deliveries') IS NOT NULL THEN RAISE EXCEPTION 'delivery table retained'; END IF;
 IF NOT EXISTS(SELECT 1 FROM official_channel_controls WHERE credential_scope='paused-scope' AND paused) THEN RAISE EXCEPTION 'expired explicit pause lost'; END IF;
 IF EXISTS(SELECT 1 FROM official_channel_controls WHERE credential_scope='resumed-scope' AND paused) THEN RAISE EXCEPTION 'resumed channel paused again'; END IF;
 IF EXISTS(SELECT 1 FROM official_channel_health) THEN RAISE EXCEPTION 'migration invented health'; END IF;
 IF EXISTS(SELECT 1 FROM information_schema.columns WHERE table_name='mobile_v9_task_drafts' AND column_name='stage') THEN RAISE EXCEPTION 'business stage retained'; END IF;
END $$;
-- Health updates intentionally have no write path to controls.
INSERT INTO official_channel_health(credential_scope,corp_id,generation,available,checked_at,last_success_at)
 VALUES('paused-scope','corp',1,true,now(),now());
DO $$ BEGIN
 IF NOT (SELECT paused FROM official_channel_controls WHERE credential_scope='paused-scope') THEN RAISE EXCEPTION 'health unpaused control'; END IF;
END $$;
CREATE TEMP TABLE mobile_v9_send_inspections (
 id integer PRIMARY KEY,binding_id uuid,dependencies jsonb,state text,started_at timestamptz,
 finished_at timestamptz,created_at timestamptz NOT NULL DEFAULT now(),lease_fence bigint,result jsonb
);
INSERT INTO mobile_v9_send_inspections(id,binding_id,dependencies,state,started_at,finished_at,lease_fence)
 SELECT n,'00000000-0000-4000-8000-000000000009','{}',s,CASE WHEN n IN (2,4) THEN now() END,CASE WHEN n=4 THEN now() END,CASE WHEN n IN (2,4) THEN 1 END
 FROM (VALUES(1,'pending'),(2,'leased'),(3,'stale'),(4,'passed'),(5,'failed')) v(n,s);
"""
source = (root / 'backend/src/distribution/south/adapter/postgres/shared_send_v2/inspection_budget.rs').read_text()
query = json.loads(re.search(r'sqlx::query_as\(("[^\n]+")\)', source).group(1))
query = query.replace('$1', "'00000000-0000-4000-8000-000000000009'::uuid").replace('$2', "'{}'::jsonb").replace('$3', '300')
checks += f'CREATE TEMP VIEW actual_budget AS {query};\n'
checks += """
DO $$ BEGIN
 IF (SELECT used FROM actual_budget)<>0 THEN RAISE EXCEPTION 'unexecuted work consumed failure budget'; END IF;
END $$;
UPDATE mobile_v9_send_inspections SET state='failed',started_at=now(),finished_at=now() WHERE id=2;
DO $$ BEGIN
 IF (SELECT used FROM actual_budget)<>1 THEN RAISE EXCEPTION 'actual failure not counted once'; END IF;
 IF NOT (SELECT retry_at>now() AND budget_at>retry_at FROM actual_budget) THEN RAISE EXCEPTION 'backoff and budget recovery incorrect'; END IF;
END $$;
UPDATE mobile_v9_send_inspections SET state='failed' WHERE id=2;
DO $$ BEGIN
 IF (SELECT used FROM actual_budget)<>1 THEN RAISE EXCEPTION 'duplicate result counted twice'; END IF;
END $$;
UPDATE mobile_v9_send_inspections SET state='stale' WHERE id=2;
DO $$ BEGIN
 IF (SELECT used FROM actual_budget)<>0 THEN RAISE EXCEPTION 'resource change counted as failure'; END IF;
END $$;
UPDATE mobile_v9_send_inspections SET state='failed',finished_at=now()-interval '25 hours' WHERE id=2;
DO $$ BEGIN
 IF (SELECT used FROM actual_budget)<>0 THEN RAISE EXCEPTION 'expired failure budget not restored'; END IF;
END $$;
UPDATE mobile_v9_send_inspections SET started_at=NULL,finished_at=NULL,lease_fence=2 WHERE id=2;
DO $$ BEGIN
 IF (SELECT used FROM actual_budget)<>0 THEN RAISE EXCEPTION 'claim without native execution counted'; END IF;
END $$;
UPDATE mobile_v9_send_inspections SET result='{"checks":{"application_opened":true}}' WHERE id=2;
DO $$ BEGIN
 IF (SELECT used FROM actual_budget)<>1 THEN RAISE EXCEPTION 'historical actual UI observation lost'; END IF;
END $$;
"""
# Exercise the exact retirement UPDATE against claim/invocation recovery facts.
tasks_source=(root/'backend/src/distribution/south/adapter/postgres/shared_send_v2/tasks.rs').read_text()
retirement=next(json.loads(raw) for raw in re.findall(r'sqlx::query\(("[^\n]+")\)',tasks_source) if "invalidation_reason='shared_send_execution_configuration_invalid'" in raw)
checks += """
CREATE TEMP TABLE mobile_v9_attempts(id uuid PRIMARY KEY,current boolean,state text,revision bigint DEFAULT 1,invalidation_reason text,invalidated_at timestamptz);
CREATE TEMP TABLE mobile_v9_send_claims(attempt_id uuid);
CREATE TEMP TABLE mobile_v9_invocations(attempt_id uuid);
INSERT INTO mobile_v9_attempts(id,current,state) SELECT ('00000000-0000-4000-8000-'||lpad(n::text,12,'0'))::uuid,n<>4,'queued' FROM generate_series(1,4) n;
INSERT INTO mobile_v9_send_claims VALUES('00000000-0000-4000-8000-000000000002');
INSERT INTO mobile_v9_invocations VALUES('00000000-0000-4000-8000-000000000003');
"""
for n in range(1,5):
    checks+=retirement.replace('$1',f"'00000000-0000-4000-8000-{n:012}'::uuid")+';\n'
checks+="""
DO $$ BEGIN
 IF (SELECT count(*) FROM mobile_v9_attempts WHERE state='invalidated' AND invalidated_at IS NOT NULL)<>1 THEN RAISE EXCEPTION 'retirement touched claimed, invoked or noncurrent work'; END IF;
 IF NOT EXISTS(SELECT 1 FROM mobile_v9_attempts WHERE id='00000000-0000-4000-8000-000000000001' AND state='invalidated') THEN RAISE EXCEPTION 'unclaimed malformed task was not retired'; END IF;
END $$;
"""
with tempfile.TemporaryDirectory(prefix='keeper-runtime-isolation-') as tmp:
    data = Path(tmp) / 'data'
    subprocess.run([str(args.postgres_bin/'initdb'), '-D', str(data), '-U', 'runtime_isolation', '--auth=trust', '--no-locale', '--encoding=UTF8'], check=True, capture_output=True)
    script = '\n\n'.join([*(p.read_text() for p in pre), seed, *(p.read_text() for p in post), checks]) + '\n\n'
    result = subprocess.run([str(args.postgres_bin/'postgres'), '--single', '-j', '-D', str(data), 'template1'], input=script, text=True, capture_output=True)
    if result.returncode or 'ERROR:' in result.stderr or 'FATAL:' in result.stderr:
        raise SystemExit(result.stdout+'\n'+result.stderr)
print('PASS: actual budget query, pending/leased/stale exclusion, once-only executed failure, backoff/recovery, claim-without-execution exclusion, actual UI observations, protected task retirement, expired pause migration, no fabricated health. No sockets or external sends.')
