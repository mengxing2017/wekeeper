#!/usr/bin/env python3
"""Smoke-test documentation from the deployed Web origin (standard library only)."""
import base64
import hashlib
import os
import re
from urllib.error import HTTPError
from urllib.request import urlopen


def check(origin):
    origin = origin.rstrip('/')

    def get(path, expected=200):
        try:
            response = urlopen(origin + path, timeout=15)
        except HTTPError as error:
            response = error
        with response:
            body = response.read().decode('utf-8')
            assert response.status == expected, (path, response.status, expected)
            return body, response.headers

    for path in ('/docs/', '/docs/guide/operator/', '/docs/guide/platform-admin/',
                 '/docs/guide/operator/mobile-tasks', '/docs/development/docs-publishing',
                 '/docs/guide/quick-start'):
        body, headers = get(path)
        assert '<div id="root"></div>' not in body, path
        assert '/docs/assets/' in body, path
        assert 'no-cache' in headers.get('Cache-Control', ''), path
        policies = headers.get_all('Content-Security-Policy') or []
        assert len(policies) == 1, (path, policies)
        policy = policies[0]
        script_policy = next(part for part in policy.split(';') if part.strip().startswith('script-src'))
        assert "'unsafe-inline'" not in script_policy, path
        for attrs, script in re.findall(r'<script\b([^>]*)>([\s\S]*?)</script>', body, re.I):
            if re.search(r'\bsrc\s*=', attrs, re.I) or not script.strip():
                continue
            digest = base64.b64encode(hashlib.sha256(script.encode()).digest()).decode()
            assert "'sha256-" + digest + "'" in script_policy, (path, 'CSP hash mismatch')
        scripts = re.findall(r'<script[^>]+src="(/docs/assets/[^"?]+\.js)"', body)
        assert scripts, path
        for script_path in scripts:
            _, script_headers = get(script_path)
            assert 'immutable' in script_headers.get('Cache-Control', ''), script_path
    with urlopen(origin + '/docs', timeout=15) as response:
        assert response.url == origin + '/docs/', response.url
    body, _ = get('/docs/this-document-does-not-exist', 404)
    assert '404' in body and '<div id="root"></div>' not in body
    _, headers = get('/admin')
    script_policy = next(part for part in headers['Content-Security-Policy'].split(';')
                         if part.strip().startswith('script-src'))
    assert script_policy.strip() == "script-src 'self'", script_policy
    print('Docs checks passed: clean URLs, assets, redirect, real 404, CSP hashes and application isolation.')


if __name__ == '__main__':
    check(os.environ.get('WEKEEPER_SMOKE_ORIGIN', 'http://127.0.0.1:8088'))
