#!/usr/bin/env python3
"""Validate real acceptance artifacts and render reviewable SQL. Never connects to a database or sends messages."""
import argparse
import hashlib
import json
from pathlib import Path
from datetime import datetime, timezone
from uuid import UUID, uuid4
from zipfile import ZipFile


def sha(path):
    h = hashlib.sha256()
    with path.open('rb') as f:
        for chunk in iter(lambda: f.read(1024 * 1024), b''):
            h.update(chunk)
    return h.hexdigest()


def validate(manifest, directory):
    required = {'device_id', 'school_id', 'target_id', 'release_id', 'environment_snapshot_id', 'reviewer_id', 'stage', 'actual_apk', 'artifacts', 'preflight_passed', 'expires_at'}
    if not required <= manifest.keys():
        raise ValueError('Missing required acceptance fields')
    for key in required & {'device_id', 'school_id', 'target_id', 'release_id', 'environment_snapshot_id', 'reviewer_id'}:
        if UUID(manifest[key]).int == 0:
            raise ValueError('Empty identity: ' + key)
    if manifest['stage'] not in ('test_send', 'production_allowlist') or manifest['preflight_passed'] is not True:
        raise ValueError('Real preflight and an explicit stage are required')
    expiry = datetime.fromisoformat(manifest['expires_at'].replace('Z', '+00:00'))
    if expiry.tzinfo is None or expiry <= datetime.now(timezone.utc):
        raise ValueError('Acceptance scope must have a future explicit expiry')
    if not manifest['artifacts']:
        raise ValueError('No real evidence artifacts')
    for artifact in [manifest['actual_apk'], *manifest['artifacts']]:
        path = (directory / artifact['path']).resolve()
        if not path.is_relative_to(directory.resolve()) or not path.is_file() or path.stat().st_size == 0:
            raise ValueError('Evidence missing or outside bundle')
        if sha(path) != artifact['sha256']:
            raise ValueError('Artifact digest mismatch: ' + artifact['path'])
    if not manifest['actual_apk']['path'].endswith('.apk'):
        raise ValueError('Actual installed APK artifact is required')
    with ZipFile(directory / manifest['actual_apk']['path']) as apk:
        if not {'AndroidManifest.xml', 'classes.dex'} <= set(apk.namelist()):
            raise ValueError('Invalid APK artifact')
    if manifest['stage'] == 'production_allowlist':
        if manifest.get('soak_hours', 0) < 72 or manifest.get('soak_passed') is not True or manifest.get('historical_debt_closed') is not True:
            raise ValueError('72-hour soak and historical debt closure required')
        if any(manifest.get('cases', {}).get(f'S{i:02}') != 'passed' for i in range(1, 41)):
            raise ValueError('All S01-S40 cases must have real passed evidence')
        refs = manifest.get('case_evidence', {})
        known = {a['sha256'] for a in manifest['artifacts']}
        if any(not refs.get(f'S{i:02}') or any(v not in known for v in refs[f'S{i:02}']) for i in range(1, 41)):
            raise ValueError('Each acceptance case must reference verified evidence')
    return manifest


def render(m):
    canonical = json.dumps(m, sort_keys=True, separators=(',', ':'), ensure_ascii=False)
    quote = lambda v: "'" + str(v).replace("'", "''") + "'"
    acceptance, scope = str(uuid4()), str(uuid4())
    fields = ['device_id', 'school_id', 'target_id', 'release_id', 'environment_snapshot_id', 'stage']
    identities = ','.join(quote(m[k]) for k in fields)
    return ('-- Review real-device evidence and identities before applying this SQL.\nBEGIN;\n'
            'SELECT pg_advisory_xact_lock(78234610243);\n'
            'INSERT INTO mobile_shared_acceptance_runs(id,device_id,school_id,target_id,release_id,environment_snapshot_id,stage,actual_apk_sha256,evidence_digest,evidence,reviewer_id) VALUES('
            + ','.join([quote(acceptance), identities, quote(m['actual_apk']['sha256']), quote(hashlib.sha256(canonical.encode()).hexdigest()), quote(canonical) + '::jsonb', quote(m['reviewer_id'])]) + ');\n'
            'INSERT INTO mobile_shared_release_scopes(id,device_id,school_id,target_id,release_id,environment_snapshot_id,stage,acceptance_id,expires_at) VALUES('
            + ','.join([quote(scope), identities, quote(acceptance), quote(m['expires_at'])]) + ');\nCOMMIT;\n')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('manifest', type=Path)
    parser.add_argument('--output', required=True, type=Path)
    args = parser.parse_args()
    m = validate(json.loads(args.manifest.read_text()), args.manifest.parent)
    args.output.write_text(render(m))
    print('Evidence hashes verified. SQL rendered for review; no database changes or sends performed.')


if __name__ == '__main__':
    main()
