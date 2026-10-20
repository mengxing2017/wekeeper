#!/bin/sh
set -eu

base_url="${WEKEEPER_SMOKE_ORIGIN:-http://127.0.0.1:8088}"
temp_dir="$(mktemp -d)"
trap 'rm -rf "$temp_dir"' EXIT HUP INT TERM

curl --fail --silent --show-error "$base_url/healthz" > "$temp_dir/health.json"
curl --fail --silent --show-error "$base_url/readyz" > "$temp_dir/ready.json"
curl --fail --silent --show-error "$base_url/api/v1/system" > "$temp_dir/system.json"
curl --fail --silent --show-error "$base_url/admin" > "$temp_dir/admin.html"
curl --fail --silent --show-error "$base_url/wecom" > "$temp_dir/wecom.html"
status="$(curl --silent --show-error -o "$temp_dir/unknown.json" -w '%{http_code}' "$base_url/api/v1/posts")"
test "$status" = 404

python3 - "$temp_dir" <<'PY'
import json
import sys
from pathlib import Path
root = Path(sys.argv[1])
assert json.loads((root / 'health.json').read_text()) == {'status': 'ok'}
assert json.loads((root / 'ready.json').read_text()) == {'status': 'ready'}
info = json.loads((root / 'system.json').read_text())['data']
assert info['application'] == 'wekeeper' and info['stage'] == 'business'
assert json.loads((root / 'unknown.json').read_text())['error']['code'] == 'not_found'
for name in ('admin', 'wecom'):
    assert '<div id="root"></div>' in (root / f'{name}.html').read_text()
print('Smoke checks passed: readiness, API contract, SPA routes and rejected unscoped routes.')
PY
