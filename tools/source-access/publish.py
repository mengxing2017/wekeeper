#!/usr/bin/env python3
"""Independent stdlib reference signer. Only deterministic public test keys are used."""
from pathlib import Path
import base64
import hashlib
import hmac
import json

root = Path(__file__).resolve().parents[2] / 'api-contract'
material = bytes(range(32))
identity = '00000000-0000-0000-0000-000000000001'
nonce = '00000000-0000-4000-8000-000000000001'
attributes = dict(event_id=identity, schema_version=2, route_id='fixture-route',
                  event_type='content.upsert', external_content_id='post-1', version='1',
                  sequence=1, occurred_at='2026-11-08T00:00:00Z', title='通知🌸',
                  content='中文正文', detail_url='https://forum.example.com/community/a/posts/1')
requests = [('unicode', 'POST', '/api/v2/source-events', dict(data=dict(type='source-events', id=identity, attributes=attributes))),
            ('get', 'GET', '/api/v2/source-receipts/00000000-0000-0000-0000-000000000002', None),
            ('auth', 'POST', '/api/v2/source-authentications', dict(data=dict(type='source-authentications', id=identity, attributes={}))),
            ('routes', 'POST', '/api/v2/source-route-checks', dict(data=dict(type='source-route-checks', id=identity, attributes=dict(limit=1)))),
            ('withdrawn', 'POST', '/api/v2/source-events', dict(data=dict(type='source-events', id=identity, attributes={**attributes, 'event_type': 'content.withdrawn', 'title': None, 'content': None})))]
vectors = []
for name, method, path, document in requests:
    body = '' if document is None else json.dumps(document, ensure_ascii=False, separators=(',', ':'))
    digest = hashlib.sha256(body.encode()).hexdigest()
    canonical = '\n'.join(['2', 'fixture-key', method, path, '1794096000', nonce, identity, digest])
    vectors.append(dict(name=name, key_id='fixture-key', method=method, path=path,
                        timestamp='1794096000', nonce=nonce, identity=identity,
                        secret=base64.urlsafe_b64encode(material).decode().rstrip('='),
                        secret_encoding='base64url-no-padding', body=body,
                        body_base64=base64.b64encode(body.encode()).decode(), digest=digest,
                        canonical=canonical, canonical_base64=base64.b64encode(canonical.encode()).decode(),
                        signature=hmac.new(material, canonical.encode(), 'sha256').hexdigest()))
(root / 'fixtures/source-v2.json').write_text(json.dumps(dict(release='source-v2-1', vectors=vectors), ensure_ascii=False, indent=2)+'\n')
manifest = dict(release='source-v2-1', signature_versions=[2], schema_versions=[2],
                secret_encoding='base64url-no-padding', secret_min_bytes=32, max_body_bytes=1048576,
                timestamp_window_seconds=300, canonical_lines=['version','key_id','METHOD','path','timestamp','nonce','idempotency_key','body_sha256_lower_hex'],
                canonical_trailing_lf=False, machine_paths=[r[2] for r in requests[:4]],
                fixtures_sha256=hashlib.sha256((root / 'fixtures/source-v2.json').read_bytes()).hexdigest())
manifest['files_sha256']={str(p.relative_to(root)):hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted([*root.glob('fixtures/source-*-v2.json'),root/'fixtures/source-v2.json',*root.glob('source-v2/*.schema.json'),root/'source-v2/errors.json']) if p.exists()}
(root / 'source-v2/manifest.json').write_text(json.dumps(manifest, indent=2)+'\n')
