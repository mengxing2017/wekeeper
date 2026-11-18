#!/usr/bin/env python3
"""Run business integration tests against an isolated, disposable PostgreSQL cluster."""
import os
import secrets
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
        env = {**os.environ, 'WEKEEPER_SHARED_SEND_KEY': os.urandom(32).hex(), 'WEKEEPER_SOURCE_MASTER_KEY_V1': secrets.token_urlsafe(32), 'WEKEEPER_TEST_DATABASE_URL': f'postgres://wekeeper_test@127.0.0.1:{port}/wekeeper_test'}
        subprocess.run(['cargo', 'test', '--locked', '--manifest-path', 'backend/Cargo.toml', '--', '--ignored', '--nocapture'], cwd=repo, env=env, check=True)

    finally:
        if started:
            subprocess.run([str(pg/'pg_ctl'), '-D', str(db), '-m', 'fast', '-w', 'stop'], check=True)
