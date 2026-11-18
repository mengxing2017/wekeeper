#!/usr/bin/env python3
"""Validate receiving policies and unchanged phone/source contracts in disposable PostgreSQL."""
import os,subprocess,tempfile,socket
from pathlib import Path
repo = Path(__file__).resolve().parents[1]
pg=Path(os.environ.get('WEKEEPER_TEST_PG_BIN', str(Path.home()/'.theseus/postgresql/17.11.0/bin')))
if not (pg/'initdb').exists():
 raise SystemExit('Set WEKEEPER_TEST_PG_BIN to PostgreSQL 17 bin, or first run make dev-embedded.')
with socket.socket() as sock:
 sock.bind(('127.0.0.1',0)); port=sock.getsockname()[1]
with tempfile.TemporaryDirectory(prefix='wekeeper-policy-') as root:
 subprocess.run([str(pg/'initdb'),'-D',root+'/db','-U','wekeeper_test','--auth=trust','--no-locale','--encoding=UTF8'],check=True,stdout=subprocess.DEVNULL)
 subprocess.run([str(pg/'pg_ctl'),'-D',root+'/db','-l',root+'/log','-o',f'-h 127.0.0.1 -p {port} -k {root}','-w','start'],check=True,stdout=subprocess.DEVNULL)
 try:
  subprocess.run([str(pg/'createdb'),'-h','127.0.0.1','-p',str(port),'-U','wekeeper_test','wekeeper_test'],check=True)
  env={**os.environ,'WEKEEPER_SHARED_SEND_KEY':os.urandom(32).hex(),'WEKEEPER_TEST_DATABASE_URL':f'postgres://wekeeper_test@127.0.0.1:{port}/wekeeper_test'}
  subprocess.run(['cargo','test','--no-fail-fast','--manifest-path','backend/Cargo.toml','--test','receiving_configurations','--test','message_orchestration','--test','source_routing','--test','shared_mobile_v9','--','--ignored','--test-threads=1'],cwd=repo,env=env,check=True)
 finally:
  subprocess.run([str(pg/'pg_ctl'),'-D',root+'/db','-m','fast','-w','stop'],check=True,stdout=subprocess.DEVNULL)
