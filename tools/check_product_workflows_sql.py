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
 "backend/src/distribution/south/adapter/postgres/shared_send_v2/usage_budgets.rs",
 "backend/src/distribution/south/adapter/postgres/shared_send_v2/wait_states.rs",
 "backend/src/distribution/south/adapter/in_process.rs",
 "backend/src/distribution/south/adapter/postgres.rs",
 "backend/src/distribution/south/adapter/postgres/groups.rs",
 "backend/src/audit/south/adapter/postgres.rs",
]
sources += [str(p.relative_to(r)) for p in (r/'backend/src/distribution/south/adapter/postgres/shared_send_v2').glob('*.rs') if str(p.relative_to(r)) not in sources]
sources += ['backend/src/distribution/south/adapter/postgres/shared_mobile_resources.rs', 'backend/src/distribution/south/adapter/postgres/mobile_workspaces.rs']
sources += ['backend/src/distribution/south/adapter/postgres/wecom_workspaces.rs', 'backend/src/distribution/south/adapter/postgres/senders.rs', 'backend/src/identity/south/adapter/postgres.rs', 'backend/src/integrations/south/adapter/assets_postgres.rs']
sources += [str(p.relative_to(r)) for p in (r/'backend/src').rglob('*.rs') if str(p.relative_to(r)) not in sources and not p.name.endswith('_tests.rs')]
queries=[]
for source in sources:
 patterns=[r'sqlx::query(?:_scalar|_as)?(?:::[^\(]*?)?\(\s*("(?:[^"\\]|\\.)*"|r#"[\s\S]*?"#)', r'const\s+\w+\s*:\s*&str\s*=\s*("(?:[^"\\]|\\.)*"|r#"[\s\S]*?"#)']
 for pattern in patterns:
  for match in re.finditer(pattern, (r/source).read_text()):
   raw=match.group(1)
   sql=raw[3:-2] if raw.startswith('r#') else json.loads(raw, strict=False)
   if sql.lstrip().upper().startswith(("SELECT", "INSERT", "UPDATE", "DELETE", "WITH")) and sql not in queries:queries.append(sql)
s='00000000-0000-4000-8000-000000000001';a='00000000-0000-4000-8000-000000000002';banner='00000000-0000-4000-8000-000000000003';default='00000000-0000-4000-8000-000000000004';expired='00000000-0000-4000-8000-000000000005';preview='00000000-0000-4000-8000-000000000006'
files=sorted((r/'backend/migrations').glob('*.sql'))
schema_parts=[]
for migration in files:
 if migration.name == '202611080125_mobile_workspaces.sql':
  schema_parts.append((r/'backend/tests/sql/mobile_workspace_migration_seed.sql').read_text())
 if migration.name == '202611080140_retire_unused_tables.sql':
  # Test the earlier transition while the historical tables still exist, then
  # exercise final retirement against populated data before live-query checks.
  schema_parts += [(r/'backend/tests/sql/mobile_workspace_independence.sql').read_text(), (r/'backend/tests/sql/wecom_workspace_independence.sql').read_text(), (r/'backend/tests/sql/retired_tables_seed.sql').read_text()]
  # An unlisted dependent table must fail atomically, preserving even earlier DDL.
  schema_parts.append("""
CREATE TABLE retirement_dependency_probe(id uuid REFERENCES posts(id));
DO $probe$ BEGIN
 BEGIN
  EXECUTE $retirement$""" + migration.read_text().replace("\n\n", "\n") + """$retirement$;
  RAISE EXCEPTION 'retirement unexpectedly deleted an unlisted dependency';
 EXCEPTION WHEN dependent_objects_still_exist THEN NULL;
 END;
 IF to_regclass('posts') IS NULL OR to_regclass('schools') IS NULL OR NOT EXISTS(SELECT 1 FROM pg_attribute WHERE attrelid='accounts'::regclass AND attname='bound_school_id' AND NOT attisdropped) OR NOT EXISTS(SELECT 1 FROM pg_trigger WHERE tgrelid='posts'::regclass AND tgname='forum_distribution_material_changed') THEN RAISE EXCEPTION 'failed retirement did not roll back atomically'; END IF;
END $probe$;
DROP TABLE retirement_dependency_probe;
""")
 schema_parts.append('BEGIN;\n' + migration.read_text() + '\nCOMMIT;')
schema='\n\n'.join(schema_parts)
seed=f"INSERT INTO wecom_workspaces(id,name) VALUES('{s}','隔离分发'); INSERT INTO accounts(id,username,display_name) VALUES('{a}','isolated-test','测试');"
checks=f"""
DO $$ BEGIN
 IF to_regclass('official_channel_deliveries') IS NOT NULL OR (SELECT count(*) FROM official_channel_health)<>0 THEN RAISE EXCEPTION 'migration retained delivery gate or fabricated health';END IF;
 IF to_regclass('official_channel_legacy') IS NOT NULL OR EXISTS(SELECT 1 FROM information_schema.columns WHERE table_name='school_resources' AND column_name IN ('url','needs_review')) THEN RAISE EXCEPTION 'obsolete compatibility schema retained';END IF;
END $$;
INSERT INTO product_previews(id,actor_id,school_id,kind,payload,digest) VALUES('{preview}','{a}','{s}','template','{{}}','digest');
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
# Exercise the real audit repository query across scoped and unscoped events.
audit_query=next(q for q in queries if q.startswith("SELECT to_jsonb(e) - 'school_id'"))
audit_page=audit_query.replace('$1', "'%isolated-system-audit%'").replace('$2','2').replace('$3','0')
audit_next=audit_query.replace('$1', "'%isolated-system-audit%'").replace('$2','2').replace('$3','2')
checks += f"""
INSERT INTO audit_events(id,school_id,actor_id,action,object_id,request_id,created_at) VALUES
 ('00000000-0000-4000-8000-000000000091','{s}','{a}','isolated.audit.scoped','{a}','isolated-system-audit','2026-11-08T00:00:00Z'),
 ('00000000-0000-4000-8000-000000000092',NULL,'{a}','isolated.audit.system','{a}','isolated-system-audit','2026-11-08T00:00:00Z'),
 ('00000000-0000-4000-8000-000000000093',NULL,'{a}','isolated.audit.latest','{a}','isolated-system-audit','2026-11-08T00:01:00Z');
CREATE TEMP TABLE audit_first_page(event) AS {audit_page};
CREATE TEMP TABLE audit_next_page(event) AS {audit_next};
DO $$ BEGIN
 IF (SELECT count(*) FROM audit_first_page)<>2 OR (SELECT count(*) FROM audit_next_page)<>1 THEN RAISE EXCEPTION 'system audit pagination failed'; END IF;
 IF EXISTS(SELECT 1 FROM audit_first_page WHERE event ? 'school_id') THEN RAISE EXCEPTION 'system audit exposes workspace scope'; END IF;
 IF NOT EXISTS(SELECT 1 FROM audit_first_page WHERE event->>'action'='isolated.audit.system') OR NOT EXISTS(SELECT 1 FROM audit_next_page WHERE event->>'action'='isolated.audit.scoped') THEN RAISE EXCEPTION 'system audit lost scoped or unscoped events'; END IF;
END $$;
"""
# Execute the actual administrator repository mutations and search projection.
def bound_sql(query, *values):
 return re.sub(r'\$(\d+)', lambda match: values[int(match.group(1))-1], query)
admin_create=next(q for q in queries if q.startswith("INSERT INTO accounts") and 'ON CONFLICT(username)' in q)
admin_edit=next(q for q in queries if q.startswith("UPDATE accounts SET display_name=$2,active=$3"))
admin_remove=next(q for q in queries if q.startswith("UPDATE accounts SET platform_admin=false"))
admin_search=next(q for q in queries if 'FROM accounts WHERE platform_admin AND' in q)
admin_id="'00000000-0000-4000-8000-000000000094'"
create_sql=bound_sql(admin_create,admin_id,"'isolated-admin'","'管理员姓名'","'hash-only'")
edit_sql=bound_sql(admin_edit,admin_id,"'新管理员姓名'",'false')
remove_sql=bound_sql(admin_remove,admin_id)
search_sql=bound_sql(admin_search,"'新管理员姓名'")
checks += f"""
DO $$ DECLARE result jsonb; removed uuid; BEGIN
 {create_sql} INTO result;
 IF NOT (result->>'active')::boolean OR result ? 'password_hash' THEN RAISE EXCEPTION 'administrator create projection failed'; END IF;
 {create_sql} INTO result;
 IF FOUND THEN RAISE EXCEPTION 'duplicate administrator account accepted'; END IF;
 {edit_sql} INTO result;
 IF (result->>'active')::boolean OR result->>'display_name'<>'新管理员姓名' THEN RAISE EXCEPTION 'administrator edit failed'; END IF;
 IF (SELECT count(*) FROM ({search_sql}) matches)<>1 THEN RAISE EXCEPTION 'administrator name search failed'; END IF;
 {remove_sql} INTO removed;
 IF removed IS NULL OR EXISTS(SELECT 1 FROM accounts WHERE id={admin_id} AND (active OR platform_admin)) THEN RAISE EXCEPTION 'administrator removal failed'; END IF;
 IF NOT EXISTS(SELECT 1 FROM accounts WHERE id={admin_id}) THEN RAISE EXCEPTION 'administrator history reference removed'; END IF;
 IF EXISTS({search_sql}) THEN RAISE EXCEPTION 'deleted administrator remains listed'; END IF;
END $$;
"""
with tempfile.TemporaryDirectory(prefix='keeper67-effects-') as tmp:
 db=Path(tmp)/'db';subprocess.run([str(pg/'initdb'),'-D',str(db),'-U','keeper67_test','--auth=trust','--no-locale','--encoding=UTF8'],check=True,capture_output=True)
 script='\n\n'.join([schema,seed,checks,(r/'backend/tests/sql/retired_tables_assertions.sql').read_text()]+[f'PREPARE product_query_{i} AS {q};' for i,q in enumerate(queries)])+'\n\n'
 result=subprocess.run([str(pg/'postgres'),'--single','-j','-D',str(db),'template1'],input=script,text=True,capture_output=True)
 
 if result.returncode or 'ERROR:' in result.stderr or 'FATAL:' in result.stderr:raise SystemExit(result.stdout+'\n'+result.stderr)
 print(f'Applied {len(files)} forward migrations and prepared {len(queries)} literal repository queries; isolated assertions passed for no fabricated delivery, no legacy schema, one-time preview, open-request uniqueness, immutable events, system audit search/projection/pagination, administrator create/edit/delete/search, final unused-table retirement and preserved live data. No sockets, production database or external send.')
