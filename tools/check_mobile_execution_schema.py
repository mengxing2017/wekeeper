"""Exercise phone resource migration without opening a socket or using any external DB.

Usage: python3 tools/check_mobile_execution_schema.py --postgres-bin /path/to/postgres/bin
This checks SQL/data boundaries; it does not replace HTTP and concurrent repository tests.
"""
import argparse
import json
import re
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
MIGRATION = "202611010102_mobile_execution_isolation.sql"
SEED = """
INSERT INTO schools(id,name) VALUES('00000000-0000-0000-0000-000000000001','School A'),('00000000-0000-0000-0000-000000000002','School B');
INSERT INTO accounts(id,username,display_name,platform_admin) VALUES('00000000-0000-0000-0000-000000000003','admin','Admin',true);
INSERT INTO wecom_groups(id,school_id,corp_id,chat_id,name) VALUES('00000000-0000-0000-0000-000000000004','00000000-0000-0000-0000-000000000001','corp','official-chat','Original group');
INSERT INTO broadcast_tasks(id,school_id,creator_id,content,content_hash,corp_id,generation,content_source) VALUES('00000000-0000-0000-0000-000000000005','00000000-0000-0000-0000-000000000001','00000000-0000-0000-0000-000000000003','Original content','original-hash','corp',1,'text');
INSERT INTO broadcast_batches(id,school_id,task_id,ordinal,sender,sender_account,binding_revision,state,corp_id,msgid) VALUES('00000000-0000-0000-0000-000000000010','00000000-0000-0000-0000-000000000001','00000000-0000-0000-0000-000000000005',1,'owner','00000000-0000-0000-0000-000000000003',1,'created','corp','official-msgid');
INSERT INTO broadcast_targets(id,school_id,task_id,batch_id,group_id,chat_id,group_name,group_revision,official_status) VALUES('00000000-0000-0000-0000-000000000011','00000000-0000-0000-0000-000000000001','00000000-0000-0000-0000-000000000005','00000000-0000-0000-0000-000000000010','00000000-0000-0000-0000-000000000004','official-chat','Original group',1,1);
INSERT INTO execution_devices(id,name) VALUES('00000000-0000-0000-0000-000000000006','Phone');
"""
VERIFY = """
INSERT INTO mobile_targets(id,school_id,search_text,created_by,updated_by) VALUES('00000000-0000-0000-0000-000000000007','00000000-0000-0000-0000-000000000001','手机备注  群🦦','00000000-0000-0000-0000-000000000003','00000000-0000-0000-0000-000000000003');
INSERT INTO mobile_device_school_grants(id,school_id,device_id,purpose,granted_by) VALUES('00000000-0000-0000-0000-000000000008','00000000-0000-0000-0000-000000000001','00000000-0000-0000-0000-000000000006','通知','00000000-0000-0000-0000-000000000003');
INSERT INTO mobile_device_target_bindings(id,school_id,device_id,target_id,grant_id,created_by,updated_by) VALUES('00000000-0000-0000-0000-000000000009','00000000-0000-0000-0000-000000000001','00000000-0000-0000-0000-000000000006','00000000-0000-0000-0000-000000000007','00000000-0000-0000-0000-000000000008','00000000-0000-0000-0000-000000000003','00000000-0000-0000-0000-000000000003');
DO $$ BEGIN
 IF (SELECT content FROM broadcast_tasks WHERE id='00000000-0000-0000-0000-000000000005') <> 'Original content' THEN RAISE EXCEPTION 'official task changed'; END IF;
 IF (SELECT chat_id FROM wecom_groups WHERE id='00000000-0000-0000-0000-000000000004') <> 'official-chat' THEN RAISE EXCEPTION 'official association changed'; END IF;
 IF NOT EXISTS(SELECT 1 FROM broadcast_targets x JOIN broadcast_batches b ON b.id=x.batch_id JOIN wecom_groups g ON g.id=x.group_id WHERE x.official_status=1 AND b.msgid='official-msgid' AND g.chat_id='official-chat') THEN RAISE EXCEPTION 'official result or customer-group association lost'; END IF;
 IF (SELECT count(*) FROM delivery_usage) <> 0 THEN RAISE EXCEPTION 'unexpected cross-channel projection'; END IF;
 IF EXISTS(SELECT 1 FROM pg_constraint WHERE conrelid IN ('mobile_targets'::regclass,'mobile_device_school_grants'::regclass,'mobile_device_target_bindings'::regclass,'mobile_device_consents'::regclass,'mobile_execution_flow_versions'::regclass,'mobile_execution_flow_validations'::regclass,'mobile_execution_flow_releases'::regclass,'mobile_execution_assignments'::regclass) AND confrelid IN ('wecom_groups'::regclass,'sending_accounts'::regclass,'device_account_bindings'::regclass)) THEN RAISE EXCEPTION 'phone resource depends on official or sender identity'; END IF;
 BEGIN
  UPDATE mobile_device_target_bindings SET school_id='00000000-0000-0000-0000-000000000002';
  RAISE EXCEPTION 'cross-school binding accepted';
 EXCEPTION WHEN foreign_key_violation THEN NULL; END;
END $$;
"""

CONSENT_VERIFY = """
INSERT INTO mobile_device_consents(id,school_id,device_id,grant_id,grant_revision,scope_digest,scope,revision,accepted,credential_generation)
SELECT '00000000-0000-0000-0000-000000000020',school_id,device_id,id,revision,repeat('a',64),'{}',1,true,0 FROM mobile_device_school_grants;
DO $$ BEGIN
 BEGIN
  UPDATE mobile_device_consents SET device_id='00000000-0000-0000-0000-000000000001';
  RAISE EXCEPTION 'consent allowed another device';
 EXCEPTION WHEN foreign_key_violation THEN NULL; END;
 BEGIN
  UPDATE mobile_device_consents SET revision=0;
  RAISE EXCEPTION 'consent allowed zero revision';
 EXCEPTION WHEN check_violation THEN NULL; END;
END $$;
UPDATE mobile_device_school_grants SET revision=revision+1,purpose='new purpose';
DO $$ BEGIN
 IF EXISTS(SELECT 1 FROM mobile_device_consents c JOIN mobile_device_school_grants g ON g.id=c.grant_id WHERE c.accepted AND g.enabled AND c.grant_revision=g.revision) THEN RAISE EXCEPTION 'old consent survived a grant change'; END IF;
END $$;
"""


def verify_flows(command, repository):
    """Exercise the actual assignment projection, including invalidation, without sockets."""
    flow = json.loads((ROOT / "api-contract/fixtures/mobile-execution-flow-v2.json").read_text())
    artifact = json.dumps(flow, ensure_ascii=False).replace("'", "''")
    seed = f"""
UPDATE execution_devices SET enabled=true;
UPDATE mobile_device_consents SET grant_revision=2,revision=2,scope='{{"purpose":"new purpose"}}';
INSERT INTO mobile_execution_flow_versions(id,school_id,name,flow,content_digest,created_by) VALUES('00000000-0000-0000-0000-000000000030','00000000-0000-0000-0000-000000000001','Test flow','{artifact}',repeat('b',64),'00000000-0000-0000-0000-000000000003');
INSERT INTO mobile_execution_flow_validations(id,school_id,flow_version_id,device_id,grant_id,grant_revision,consent_id,consent_revision,search_text,challenge,state,credential_generation,created_by,completed_at)
SELECT '00000000-0000-0000-0000-000000000031',school_id,'00000000-0000-0000-0000-000000000030',device_id,grant_id,grant_revision,id,revision,'验证群','device-only-challenge','passed',0,'00000000-0000-0000-0000-000000000003',clock_timestamp() FROM mobile_device_consents;
INSERT INTO mobile_execution_flow_releases(id,school_id,flow_version_id,validation_id,name,created_by) VALUES('00000000-0000-0000-0000-000000000032','00000000-0000-0000-0000-000000000001','00000000-0000-0000-0000-000000000030','00000000-0000-0000-0000-000000000031','Release','00000000-0000-0000-0000-000000000003');
INSERT INTO mobile_execution_assignments(id,school_id,device_id,grant_id,release_id,created_by,updated_by) SELECT '00000000-0000-0000-0000-000000000033',school_id,device_id,grant_id,'00000000-0000-0000-0000-000000000032','00000000-0000-0000-0000-000000000003','00000000-0000-0000-0000-000000000003' FROM mobile_device_consents;
DO $$ BEGIN
 BEGIN
  UPDATE mobile_execution_assignments SET school_id='00000000-0000-0000-0000-000000000002';
  RAISE EXCEPTION 'cross-school flow assignment accepted';
 EXCEPTION WHEN foreign_key_violation THEN NULL; END;
END $$;
"""
    query = re.search(r'Resource::Assignments => \{\s*"([^"\n]+)"', repository.read_text()).group(1)
    for key, value in {'$1': "'00000000-0000-0000-0000-000000000001'::uuid", '$2': "''::text", '$3': 'NULL::uuid', '$4': '31', '$5': '0'}.items():
        query = query.replace(key, value)
    def check(expected, message):
        return f"DO $$ DECLARE found jsonb; BEGIN SELECT value INTO found FROM ({query}) q(value); IF found IS NULL OR (found->>'available')::boolean IS DISTINCT FROM {str(expected).lower()} THEN RAISE EXCEPTION '{message}'; END IF; END $$;"
    verify = seed + check(True, 'valid current assignment not available')
    verify += "UPDATE mobile_execution_flow_releases SET enabled=false;" + check(False, 'revoked release available')
    verify += "UPDATE mobile_execution_flow_releases SET enabled=true; UPDATE mobile_device_school_grants SET revision=3;" + check(False, 'stale consent available')
    verify += "UPDATE mobile_device_consents SET grant_revision=3,revision=3;" + check(False, 'stale device validation available')
    verify += "UPDATE mobile_device_school_grants SET revision=2; UPDATE mobile_device_consents SET grant_revision=2,revision=2; UPDATE mobile_execution_flow_validations SET state='failed';" + check(False, 'failed validation available')
    verify += "UPDATE mobile_execution_flow_validations SET state='passed'; UPDATE execution_devices SET credential_generation=1;" + check(False, 'old credential generation validation available')
    run(command, input=verify + "\n\n")


def verify_tasks(command):
    """Exercise durable identity/immutability/physical reservation constraints."""
    source = (ROOT / 'backend/src/distribution/south/adapter/postgres/mobile_execution/tasks.rs').read_text()
    configuration = re.search(r'"(SELECT jsonb_build_object\(\'device_id[^"\n]+)"', source).group(1)
    for key,value in {'$1': "'00000000-0000-0000-0000-000000000001'::uuid", '$2': "'00000000-0000-0000-0000-000000000006'::uuid"}.items():
        configuration = configuration.replace(key,value)
    def config_check(expected, message):
        return f"DO $$ DECLARE found jsonb; BEGIN SELECT value INTO found FROM ({configuration}) q(value); IF (found IS NOT NULL) IS DISTINCT FROM {str(expected).lower()} THEN RAISE EXCEPTION '{message}'; END IF; END $$;"
    config_sql = "UPDATE execution_devices SET credential_generation=0;" + config_check(True, 'valid task configuration rejected')
    config_sql += "UPDATE mobile_device_consents SET accepted=false;" + config_check(False, 'withdrawn consent accepted for task')
    config_sql += "UPDATE mobile_device_consents SET accepted=true; UPDATE mobile_execution_flow_releases SET enabled=false;" + config_check(False, 'revoked release accepted for task')
    config_sql += "UPDATE mobile_execution_flow_releases SET enabled=true; UPDATE mobile_device_school_grants SET revision=4;" + config_check(False, 'stale grant accepted for task')
    config_sql += "UPDATE mobile_device_school_grants SET revision=2;"
    run(command, input=config_sql + "\n\n")
    uid = lambda n: f"'00000000-0000-0000-0000-{n:012d}'"
    a,b,actor,device,target,binding = (uid(n) for n in (1,2,3,6,7,9))
    p,t,intent,plan,item = (uid(n) for n in (40,41,42,43,44))
    sql = f"""
INSERT INTO mobile_execution_task_previews(id,school_id,device_id,input,snapshot,digest,created_by) VALUES({p},{a},{device},'{{}}','{{"message":"原文"}}',repeat('c',64),{actor});
INSERT INTO mobile_execution_tasks(id,school_id,intent_id,device_id,preview_id,created_by,confirmed_by) VALUES({t},{a},{intent},{device},{p},{actor},{actor});
INSERT INTO mobile_execution_plans(id,school_id,task_id,snapshot,digest) VALUES({plan},{a},{t},'{{"message":"原文"}}',repeat('c',64));
INSERT INTO mobile_execution_task_items(id,school_id,task_id,intent_id,device_id,target_id,binding_id,position,snapshot) VALUES({item},{a},{t},{intent},{device},{target},{binding},1,'{{"search_text":"原始群名"}}');
INSERT INTO mobile_execution_device_reservations(device_id,school_id,task_id) VALUES({device},{a},{t});
INSERT INTO mobile_execution_task_previews(id,school_id,device_id,input,snapshot,digest,created_by) VALUES({uid(50)},{b},{device},'{{}}','{{}}',repeat('c',64),{actor});
INSERT INTO mobile_execution_tasks(id,school_id,intent_id,device_id,preview_id,created_by,confirmed_by) VALUES({uid(51)},{b},{uid(52)},{device},{uid(50)},{actor},{actor});
DO $$ BEGIN
 BEGIN
  INSERT INTO mobile_execution_device_reservations(device_id,school_id,task_id) VALUES({device},{b},{uid(51)});
  RAISE EXCEPTION 'another school reserved occupied physical device';
 EXCEPTION WHEN unique_violation THEN NULL; END;
 BEGIN
  UPDATE mobile_execution_plans SET snapshot='{{}}' WHERE id={plan};
  RAISE EXCEPTION 'plan rewritten';
 EXCEPTION WHEN check_violation THEN NULL; END;
 BEGIN
  UPDATE mobile_execution_task_previews SET digest=repeat('d',64) WHERE id={p};
  RAISE EXCEPTION 'preview rewritten';
 EXCEPTION WHEN check_violation THEN NULL; END;
 BEGIN
  UPDATE mobile_execution_task_items SET snapshot='{{}}' WHERE id={item};
  RAISE EXCEPTION 'target snapshot rewritten';
 EXCEPTION WHEN check_violation THEN NULL; END;
 BEGIN
  INSERT INTO mobile_execution_device_reservations(device_id,school_id,task_id) VALUES({device},{a},{t});
  RAISE EXCEPTION 'duplicate physical reservation accepted';
 EXCEPTION WHEN unique_violation THEN NULL; END;
 BEGIN
  UPDATE mobile_execution_tasks SET school_id={b} WHERE id={t};
  RAISE EXCEPTION 'cross school task accepted';
 EXCEPTION WHEN foreign_key_violation THEN NULL; END;
 BEGIN
  INSERT INTO mobile_execution_task_items(id,school_id,task_id,intent_id,device_id,target_id,binding_id,position,snapshot) VALUES({uid(45)},{a},{t},{intent},{device},{target},{binding},2,'{{}}');
  RAISE EXCEPTION 'intent repeated target accepted';
 EXCEPTION WHEN unique_violation THEN NULL; END;
END $$;
UPDATE mobile_execution_task_items SET state='unknown' WHERE id={item};
"""
    repository = ROOT / 'backend/src/distribution/south/adapter/postgres/mobile_execution.rs'
    task_query = re.search(r'Resource::Tasks => \{\s*"([^"\n]+)"', repository.read_text()).group(1)
    for key,value in {'$1': a+'::uuid', '$2': "''::text", '$3': 'NULL::uuid', '$4': '31', '$5': '0'}.items():
        task_query = task_query.replace(key,value)
    sql += f"""DO $$ DECLARE found jsonb; BEGIN SELECT value INTO found FROM ({task_query}) q(value);
 IF found->>'device_name' IS DISTINCT FROM 'Phone' OR found->'items'->0->'snapshot'->>'search_text' IS DISTINCT FROM '原始群名' THEN RAISE EXCEPTION 'task projection lost device name or target snapshot'; END IF;
END $$;"""
    # Execute the exact cancellation predicate used by the repository: unknown must keep its reservation.
    source = (ROOT / 'backend/src/distribution/south/adapter/postgres/mobile_execution/tasks.rs').read_text()
    cancel = re.search(r'"(UPDATE mobile_execution_tasks t SET [^"\n]+)"', source).group(1)
    for key,value in {'$1': a+'::uuid', '$2': t+'::uuid', '$3': '1::bigint'}.items():
        cancel = cancel.replace(key,value)
    sql += cancel + ";"
    sql += f"""DO $$ BEGIN
 IF NOT EXISTS(SELECT 1 FROM mobile_execution_tasks WHERE id={t} AND state='queued') OR NOT EXISTS(SELECT 1 FROM mobile_execution_device_reservations WHERE device_id={device}) THEN RAISE EXCEPTION 'unknown released physical reservation'; END IF;
END $$;
UPDATE mobile_execution_task_items SET state='pending' WHERE id={item};
""" + cancel + ";"
    sql += f"""DO $$ BEGIN
 IF NOT EXISTS(SELECT 1 FROM mobile_execution_tasks WHERE id={t} AND state='cancelled') THEN RAISE EXCEPTION 'certainly unclaimed work cannot cancel'; END IF;
END $$;"""
    run(command, input=sql + "\n\n")


def run(command, **kwargs):
    result = subprocess.run(command, text=True, capture_output=True, **kwargs)
    errors = [line for line in result.stderr.splitlines() if "ERROR:" in line or "FATAL:" in line or "PANIC:" in line]
    if result.returncode or errors:
        raise RuntimeError(f"{command[0]} failed:\n{result.stderr}\n{result.stdout}")
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--postgres-bin", required=True, type=Path)
    args = parser.parse_args()
    migrations = sorted((ROOT / "backend/migrations").glob("*.sql"))
    with tempfile.TemporaryDirectory(prefix="wekeeper-phone-schema-") as directory:
        data = Path(directory) / "data"
        run([str(args.postgres_bin / "initdb"), "-D", str(data), "-A", "trust", "--no-locale", "--encoding=UTF8"])
        command = [str(args.postgres_bin / "postgres"), "--single", "-j", "-D", str(data), "postgres"]
        baseline = "\n".join(p.read_text() for p in migrations if p.name < MIGRATION)
        run(command, input=baseline + "\n\n")
        run(command, input=SEED + "\n\n")
        run(command, input="\n".join(p.read_text() for p in migrations if p.name >= MIGRATION) + "\n\n")
        run(command, input=VERIFY + CONSENT_VERIFY + "\n\n")
        repository = ROOT / "backend/src/distribution/south/adapter/postgres/mobile_execution.rs"
        queries = re.findall(r'"((?:SELECT|INSERT|UPDATE) [^"\n]*)"', "\n".join(p.read_text() for p in [repository, *repository.with_suffix("").glob("*.rs")]))
        static_queries = [json.loads('"' + query + '"') for query in queries if "{table}" not in query]
        run(command, input="\n".join(f"PREPARE phone_query_{i} AS {query};" for i, query in enumerate(static_queries)) + "\n\n")
        verify_flows(command, repository)
        verify_tasks(command)
    print("PASS: upgrade preserves official data; phone resources/flow assignments reject cross-school/device bindings, stale consent, revoked releases and invalid validation; task snapshots/intents/reservations remain constrained; repository SQL prepares successfully")


if __name__ == "__main__":
    main()
