"""Offline schema/query checks only; does not replace SQLx/concurrency integration tests.

Usage: python tools/check_product_workflows_sql.py --postgres-bin /path/to/postgresql/bin
The database is temporary and single-user. No sockets or environment DB URLs are used.
"""
from pathlib import Path
import argparse
import json
import re
import tempfile
import subprocess
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--postgres-bin", required=True, type=Path)
args = parser.parse_args()
r = Path(__file__).resolve().parents[1]
pg = args.postgres_bin
sources = [
 "backend/src/distribution/south/adapter/postgres/workflows.rs",
 "backend/src/distribution/south/adapter/postgres/product_preview.rs",
 "backend/src/distribution/south/adapter/postgres/broadcast.rs",
 "backend/src/distribution/south/adapter/postgres/resource_policy.rs",
 "backend/src/distribution/south/adapter/postgres/shared_send_v2/tasks.rs",
 "backend/src/distribution/south/adapter/postgres/shared_send_v2/usage.rs",
 "backend/src/distribution/south/adapter/postgres/shared_send_v2/inspections.rs",
 "backend/src/distribution/south/adapter/postgres/shared_send_v2/inspection_budget.rs",
 "backend/src/distribution/south/adapter/postgres/shared_send_v2/wait_states.rs",
 "backend/src/schools/south/adapter/postgres/resources.rs",
 "backend/src/schools/south/adapter/postgres.rs",
]
queries=[]
for source in sources:
 for match in re.finditer(r'sqlx::query(?:_scalar|_as)?(?:::[^\(]*?)?\(\s*("(?:[^"\\]|\\.)*"|r#"[\s\S]*?"#)', (r/source).read_text()):
  raw=match.group(1)
  sql=raw[3:-2] if raw.startswith('r#') else json.loads(raw)
  if sql.lstrip().upper().startswith(("SELECT", "INSERT", "UPDATE", "DELETE", "WITH")) and sql not in queries:queries.append(sql)
s='00000000-0000-4000-8000-000000000001';a='00000000-0000-4000-8000-000000000002';banner='00000000-0000-4000-8000-000000000003';default='00000000-0000-4000-8000-000000000004';expired='00000000-0000-4000-8000-000000000005';preview='00000000-0000-4000-8000-000000000006'
files=sorted((r/'backend/migrations').glob('*.sql'))
schema='\n'.join(p.read_text() for p in files)
seed=f"INSERT INTO schools(id,name) VALUES('{s}','隔离学校'); INSERT INTO accounts(id,username,display_name) VALUES('{a}','isolated-test','测试');"
checks=f"""
DO $$ BEGIN
 IF to_regclass('official_channel_deliveries') IS NOT NULL OR (SELECT count(*) FROM official_channel_health)<>0 THEN RAISE EXCEPTION 'migration retained delivery gate or fabricated health';END IF;
 IF to_regclass('official_channel_legacy') IS NOT NULL OR EXISTS(SELECT 1 FROM information_schema.columns WHERE table_name='school_resources' AND column_name IN ('url','needs_review')) THEN RAISE EXCEPTION 'obsolete compatibility schema retained';END IF;
END $$;
INSERT INTO school_resources(id,kind,title,active) VALUES('{default}','legal','平台默认',true);
INSERT INTO school_resources(id,school_id,kind,title,active,ends_at) VALUES('{expired}','{s}','legal','过期学校覆盖',true,now()-interval '1 second');
DO $$ BEGIN
 IF NOT EXISTS(SELECT 1 FROM school_resources r WHERE r.id='{default}' AND NOT EXISTS(SELECT 1 FROM school_resources custom WHERE custom.school_id='{s}' AND custom.kind=r.kind AND custom.active AND (custom.starts_at IS NULL OR custom.starts_at<=now()) AND (custom.ends_at IS NULL OR custom.ends_at>now()))) THEN RAISE EXCEPTION 'expired school override suppressed fallback';END IF;
END $$;
INSERT INTO product_previews(id,actor_id,school_id,kind,payload,digest) VALUES('{preview}','{a}','{s}','resource','{{}}','digest');
UPDATE product_previews SET result='{{"id":"one"}}',confirmed_at=now() WHERE id='{preview}' AND result IS NULL;
UPDATE product_previews SET result='{{"id":"two"}}',confirmed_at=now() WHERE id='{preview}' AND result IS NULL;
DO $$ BEGIN
 IF (SELECT result->>'id' FROM product_previews WHERE id='{preview}')<>'one' THEN RAISE EXCEPTION 'confirmation overwritten';END IF;
END $$;
INSERT INTO product_requests(id,actor_id,school_id,kind,context_key,payload) VALUES('{default}','{a}','{s}','channel','generation:1','{{}}');
DO $$ BEGIN
 BEGIN INSERT INTO product_requests(id,actor_id,school_id,kind,context_key,payload) VALUES('{expired}','{a}','{s}','channel','generation:1','{{}}'); RAISE EXCEPTION 'duplicate open request accepted';EXCEPTION WHEN unique_violation THEN NULL;END;
 INSERT INTO product_request_events(id,request_id,actor_id,state,response) VALUES('{preview}','{default}','{a}','pending','已提交');
 BEGIN UPDATE product_request_events SET response='改写' WHERE id='{preview}';RAISE EXCEPTION 'immutable event overwritten';EXCEPTION WHEN raise_exception THEN IF SQLERRM<>'immutable business event' THEN RAISE;END IF;END;
END $$;
"""
with tempfile.TemporaryDirectory(prefix='keeper67-effects-') as tmp:
 db=Path(tmp)/'db';subprocess.run([str(pg/'initdb'),'-D',str(db),'-U','keeper67_test','--auth=trust','--no-locale','--encoding=UTF8'],check=True,capture_output=True)
 script='\n\n'.join([schema,seed,checks]+[f'PREPARE product_query_{i} AS {q};' for i,q in enumerate(queries)])+'\n\n'
 result=subprocess.run([str(pg/'postgres'),'--single','-j','-D',str(db),'template1'],input=script,text=True,capture_output=True)
 
 if result.returncode or 'ERROR:' in result.stderr or 'FATAL:' in result.stderr:raise SystemExit(result.stdout+'\n'+result.stderr)
 print(f'Applied {len(files)} forward migrations and prepared {len(queries)} literal repository queries; isolated assertions passed for no fabricated delivery, no legacy schema, expired fallback, one-time preview, open-request uniqueness, immutable events. No sockets, production database or external send.')
