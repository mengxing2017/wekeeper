#!/usr/bin/env python3
"""Actual embedded API + Vite proxy + cookie/CSRF smoke; uses no external database."""
import http.cookiejar
import json
import os
import secrets
import signal
import socket
import subprocess
import tempfile
import time
import urllib.error
import urllib.request
from pathlib import Path

repo=Path(__file__).resolve().parents[1]
def port():
    with socket.socket() as s:s.bind(('127.0.0.1',0));return s.getsockname()[1]
def ready(url,process):
    for _ in range(150):
        if process.poll() is not None:raise RuntimeError('temporary process exited; inspect its captured log')
        try:
            with urllib.request.urlopen(url,timeout=1) as r:
                if r.status==200:return
        except OSError:pass
        time.sleep(.2)
    raise RuntimeError('temporary service did not become ready')
with tempfile.TemporaryDirectory(prefix='wekeeper-live-') as directory:
    root=Path(directory);apiport,webport=port(),port();origin=f'http://127.0.0.1:{webport}';password=secrets.token_urlsafe(24);children=[]
    with (root/'api.log').open('w+') as alog,(root/'web.log').open('w+') as wlog:
        try:
            env={**os.environ,'WEKEEPER_BIND_ADDR':f'127.0.0.1:{apiport}','WEKEEPER_PUBLIC_ORIGIN':origin,'WEKEEPER_INSECURE_LOCAL_AUTH':'true','WEKEEPER_DEV_ADMIN_PASSWORD':password}
            api=subprocess.Popen([str(repo/'backend/target/debug/examples/embedded_postgres')],env=env,cwd=repo,stdout=alog,stderr=subprocess.STDOUT);children.append(api)
            ready(f'http://127.0.0.1:{apiport}/readyz',api)
            web=subprocess.Popen(['pnpm','--dir','frontend','dev','--host','127.0.0.1','--port',str(webport),'--strictPort'],env={**os.environ,'WEKEEPER_API_ORIGIN':f'http://127.0.0.1:{apiport}'},cwd=repo,stdout=wlog,stderr=subprocess.STDOUT,start_new_session=True);children.append(web)
            ready(origin+'/readyz',web)
            jar=http.cookiejar.CookieJar();client=urllib.request.build_opener(urllib.request.HTTPCookieProcessor(jar))
            def request(path,method='GET',body=None,csrf=None,expected=200):
                headers={'Origin':origin,'Content-Type':'application/vnd.api+json'}
                if csrf:headers['X-CSRF-Token']=csrf
                req=urllib.request.Request(origin+path,data=json.dumps({'data':{'type':('sessions' if '/auth/' in path else 'posts' if path.endswith('/posts') else 'schools'),'attributes':body}}).encode() if body is not None else None,headers=headers,method=method)
                try:r=client.open(req,timeout=10)
                except urllib.error.HTTPError as e:r=e
                with r:
                    assert r.status==expected,(path,r.status,r.read().decode())
                    return json.loads(r.read())
            session=request('/api/v1/auth/sessions','POST',{'username':'admin','password':password})['data']['attributes'];csrf=session['csrf']
            assert any(c.name=='wekeeper_session' and c.has_nonstandard_attr('HttpOnly') for c in jar)
            request('/api/v1/schools','POST',{'name':'Denied'},expected=403)
            school=request('/api/v1/schools','POST',{'name':'Live smoke'},csrf)['data']['id']
            post=request(f'/api/v1/schools/{school}/posts','POST',{'title':'Live post','content':'Body'},csrf)['data']['attributes']
            assert post['audit_status']=='pending'
            for path in ('/admin','/community','/wecom'):
                with client.open(origin+path) as response:assert '<div id="root"></div>' in response.read().decode()
            request('/api/v1/auth/session','DELETE',csrf=csrf)
            request('/api/v1/schools',expected=401)
            print('Live API + Vite proxy, administrator bootstrap, Cookie/CSRF, post creation, SPA routes and logout passed.')
        except Exception:
            for log in (alog,wlog):log.flush();log.seek(0);print(log.read())
            raise
        finally:
            for p in reversed(children):
                if p.poll() is None:
                    if len(children)>1 and p is children[1]:os.killpg(p.pid,signal.SIGTERM)
                    else:p.terminate()
                    p.wait(timeout=15)
            if children and children[0].returncode!=0:raise RuntimeError('API failed to shut down cleanly')
