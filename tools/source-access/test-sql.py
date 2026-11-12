#!/usr/bin/env python3
"""Validate the full migration chain and source SQL invariants without listening sockets."""
from pathlib import Path
import os
import re
import shutil
import subprocess
import tempfile

root = Path(__file__).resolve().parents[2]
pg_bin = os.environ.get('SOURCE_TEST_PG_BIN')
def executable(name):
    return str(Path(pg_bin) / name) if pg_bin else shutil.which(name)

with tempfile.TemporaryDirectory(prefix='source-access-sql-') as directory:
    subprocess.run([executable('initdb'), '-D', directory, '-A', 'trust', '--no-locale'], check=True, stdout=subprocess.DEVNULL)
    sql = '\n'.join(p.read_text() for p in sorted((root / 'backend/migrations').glob('*.sql')))
    # Exercise the actual production list SQL against two callers, including its JSON projection.
    repository = (root / 'backend/src/source_access/south/adapter/postgres.rs').read_text()
    template = re.search(r'"(SELECT to_jsonb\(t\).*?OFFSET \$5)"', repository).group(1)
    checks = []
    for kind, pk, owner in [
        ('source_credentials', 'key_id', 't.key_id'),
        ('source_routes', 'route_id', '(SELECT c.key_id FROM source_credentials c WHERE c.source_id=t.source_id ORDER BY c.key_id LIMIT 1)'),
    ]:
        unit_projection = ("jsonb_build_object('business_unit_name',(SELECT u.name FROM business_units u WHERE u.id=t.business_unit_id))" if kind == 'source_routes' else "'{}'::jsonb")
        query = template.format(table=kind, pk=pk, owner_key=owner, unit_projection=unit_projection)
        params = {'1': 'NULL', '2': "''", '3': "''", '4': "(SELECT source_id FROM source_credentials WHERE key_id='k1')", '5': '0', '6': 'NULL', '7': 'NULL'}
        query = re.sub(r'\$(\d+)', lambda match: params[match[1]], query)
        checks.append(f"CREATE TEMP TABLE check_{kind} AS SELECT q.value FROM ({query}) AS q(value);")
        checks.append(f"""DO $$ BEGIN
            IF (SELECT count(*) FROM check_{kind})<>1 THEN RAISE EXCEPTION 'caller filtering failed: {kind}'; END IF;
            IF EXISTS(SELECT 1 FROM check_{kind} WHERE value->>'key_id'<>'k1' OR value ?| ARRAY['source_id','encrypted_secret','delivery_pending']) THEN RAISE EXCEPTION 'public credential projection failed: {kind}'; END IF;
        END $$;""")
    smoke = (root / 'backend/tests/source_v2.sql').read_text()
    smoke = smoke.replace("DELETE FROM source_key_routes WHERE key_id='k1';", '\n'.join(checks) + "\nDELETE FROM source_key_routes WHERE key_id='k1';", 1)
    sql += '\n' + smoke + '\n\n'
    result = subprocess.run([executable('postgres'), '--single', '-j', '-D', directory, 'postgres'], input=sql, text=True, capture_output=True)
    errors = [line for line in result.stderr.splitlines() if 'ERROR:' in line or 'FATAL:' in line]
    if result.returncode or errors:
        raise SystemExit('\n'.join(errors) or 'PostgreSQL single-user validation failed')
    print('Full migration chain and source SQL assertions passed.')
