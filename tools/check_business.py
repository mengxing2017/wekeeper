#!/usr/bin/env python3
"""Run business integration tests against an isolated, disposable PostgreSQL cluster."""
import os
import json
import sys
sys.dont_write_bytecode = True
import socket
import subprocess
import tempfile
from pathlib import Path

repo = Path(__file__).resolve().parents[1]
pg = Path(os.environ.get('WEKEEPER_TEST_PG_BIN', str(Path.home() / '.theseus/postgresql/17.11.0/bin')))
if not (pg / 'initdb').exists():
    raise SystemExit('Set WEKEEPER_TEST_PG_BIN to PostgreSQL 17 bin, or first run make dev-embedded.')
with socket.socket() as sock:
    sock.bind(('127.0.0.1', 0))
    port = sock.getsockname()[1]
with tempfile.TemporaryDirectory(prefix='wekeeper-business-') as directory:
    root = Path(directory)
    db = root / 'db'
    subprocess.run([str(pg/'initdb'), '-D', str(db), '-U', 'wekeeper_test', '--auth=trust', '--no-locale', '--encoding=UTF8'], check=True, stdout=subprocess.DEVNULL)
    started = False
    try:
        subprocess.run([str(pg/'pg_ctl'), '-D', str(db), '-l', str(root/'postgres.log'), '-o', f'-h 127.0.0.1 -p {port} -k {root}', '-w', 'start'], check=True)
        started = True
        subprocess.run([str(pg/'createdb'), '-h', '127.0.0.1', '-p', str(port), '-U', 'wekeeper_test', 'wekeeper_test'], check=True)
        env = {**os.environ, 'WEKEEPER_TEST_DATABASE_URL': f'postgres://wekeeper_test@127.0.0.1:{port}/wekeeper_test'}
        subprocess.run(['cargo', 'test', '--manifest-path', 'backend/Cargo.toml', '--lib', '--test', 'business', '--test', 'database', '--test', 'directory', '--test', 'assets', '--test', 'broadcast', '--', '--ignored', '--nocapture'], cwd=repo, env=env, check=True)
        sys.path.insert(0,str(repo/'tools/migration'))
        from convert import convert,identity
        source=root/'snapshot.json'
        snapshot={'tables':{'fa_school':[{'id':1,'name':'Migration school'}],'fa_user':[{'id':2,'nickname':'User','school_id':1}],'fa_discover':[{'id':3,'school_id':1,'user_id':2,'title':'Before','content':'Text','statusdata':'1','audit_status':'approved'}]}}
        source.write_text(json.dumps(snapshot))
        assert convert(source,root/'initial',{'1':'visible'})['ready_to_apply']
        psql=[str(pg/'psql'),env['WEKEEPER_TEST_DATABASE_URL'],'-v','ON_ERROR_STOP=1','-q']
        for _ in range(2):subprocess.run(psql+['-f',str(root/'initial/apply.sql')],check=True)
        changed=root/'changed.json';snapshot['tables']['fa_discover'][0]['title']='After';changed.write_text(json.dumps(snapshot))
        assert convert(changed,root/'incremental',{'1':'visible'},previous=source)['ready_to_apply']
        subprocess.run(psql+['-f',str(root/'incremental/apply.sql')],check=True)
        target=identity('fa_discover',3)
        check=subprocess.check_output(psql+['-At','-c',f"SELECT title FROM posts WHERE id='{target}'"],text=True).strip()
        assert check=='After',check
        subprocess.run(psql+['-c',f"UPDATE posts SET title='New system write',revision=revision+1 WHERE id='{target}'"],check=True)
        conflict=root/'conflict.json';snapshot['tables']['fa_discover'][0]['title']='Must not overwrite';conflict.write_text(json.dumps(snapshot))
        assert convert(conflict,root/'conflict',{'1':'visible'},previous=changed)['ready_to_apply']
        result=subprocess.run(psql+['-f',str(root/'conflict/apply.sql')],capture_output=True,text=True)
        assert result.returncode!=0,'incremental replay overwrote a new-system write'
        print('Migration import, idempotent replay, incremental replay and write-conflict rollback passed.')

    finally:
        if started:
            subprocess.run([str(pg/'pg_ctl'), '-D', str(db), '-m', 'fast', '-w', 'stop'], check=True)
