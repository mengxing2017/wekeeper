#!/usr/bin/env python3
"""Convert a read-only legacy JSON snapshot to reviewable, transactional PostgreSQL SQL.
No network or production DB access. Unknown states/references/media block the apply file.
Input: {"tables":{"fa_school":[...], ...}}. Passwords/tokens are never imported.
"""
import argparse
import base64
import hashlib
import json
import mimetypes
import os
import tempfile
import uuid
from datetime import datetime, timezone
from pathlib import Path

NS = uuid.UUID('f78c1be0-1e30-4630-bf4e-df9086cbf860')
SUPPORTED = ['fa_school','fa_user','fa_admin','fa_admin_school','fa_discover','fa_discover_comment','fa_discover_favor','fa_discover_collect','fa_discover_attentions','fa_discover_log','fa_discover_report','fa_discover_feedback','fa_home_banner','fa_school_group_qrcode','fa_discover_tag','fa_discover_blocked_word','fa_wecom_send_log','fa_third','fa_config','fa_legal_docs']

def identity(table, value):
    return str(uuid.uuid5(NS, f'{table}:{value}'))

def stamp(value):
    if value in (None, '', 0, '0'):
        return '1970-01-01T00:00:00+00:00'
    number = int(value)
    if number > 100_000_000_000:
        raise ValueError('timestamp unit is ambiguous (milliseconds are not assumed)')
    return datetime.fromtimestamp(number, timezone.utc).isoformat()

def literal(value):
    if value is None: return 'NULL'
    if isinstance(value, bool): return 'TRUE' if value else 'FALSE'
    if isinstance(value, (int,float)): return str(value)
    if isinstance(value, (dict,list)): return "'"+json.dumps(value,ensure_ascii=False).replace("'","''")+"'::jsonb"
    return "'"+str(value).replace("'","''")+"'"

def convert(source, out, states, media_root=None, media_map=None, corp_id=None, previous=None):
    raw = source.read_bytes()
    doc = json.loads(raw)
    tables = doc['tables']
    digest = hashlib.sha256(raw).hexdigest()
    batch = identity('batch',digest)
    errors, warnings, rows, mappings = [], [], [], []
    counts = {}
    lookup = {t:{str(r['id']):r for r in values} for t,values in tables.items()}
    seen = set()
    media_map = media_map or {}
    def ref(table, value):
        if str(value) not in lookup.get(table,{}): raise ValueError(f'missing {table} reference: {value}')
        return identity(table,value)
    def school(r, user=None):
        value = r.get('school_id') or (user or {}).get('school_id')
        return ref('fa_school',value)
    def add(table, values, source_table=None, old=None):
        key=(table,values.get('id') or json.dumps(values,sort_keys=True))
        if key in seen: raise ValueError(f'duplicate target {table}')
        seen.add(key);rows.append((table,values));counts[table]=counts.get(table,0)+1
        if source_table: mappings.append((source_table,str(old),values.get('id')))
    def media(paths,s,owner):
        urls=[]
        for path in paths:
            if not path: continue
            if not media_root: raise ValueError('media requires --media-root and a verified local file')
            local=(media_root/media_map.get(path,path.lstrip('/'))).resolve()
            if not local.is_relative_to(media_root.resolve()): raise ValueError('media path outside root')
            content=local.read_bytes()
            if len(content)>5*1024*1024: raise ValueError('media exceeds 5 MiB')
            mime=mimetypes.guess_type(local.name)[0]
            if mime not in ('image/png','image/jpeg','image/webp','image/gif'): raise ValueError('unsupported legacy media type')
            mid=identity('media',s+':'+path)
            if ('media_assets',mid) not in seen:
                add('media_assets',dict(id=mid,school_id=s,owner_id=owner,mime_type=mime,size_bytes=len(content),content={'__bytes':base64.b64encode(content).decode()}))
            urls.append(f'/api/v1/schools/{s}/media/{mid}')
        return urls
    for table in SUPPORTED:
        for r in tables.get(table,[]):
            old=r['id']; target=identity(table,old)
            try:
                if table=='fa_school':
                    config={k:r[k] for k in ['short_name','province','city','area','address','latitude','longitude','theme_color','theme_primary','theme_secondary','theme_text_color','logo','header_bg_image','post_audit_mode','comment_audit_mode'] if k in r}
                    config['theme_color']=r.get('theme_primary') or config.get('theme_color','#25624b')
                    add('schools',dict(id=target,name=r['name'],active=r.get('status','normal')=='normal',config=config,created_at=stamp(r.get('createtime'))),table,old)
                elif table=='fa_legal_docs':
                    add('school_resources',dict(id=target,school_id=None,kind='legal',title=r['title'],content=r['content'],url='',active=True),table,old)
                elif table=='fa_third':
                    if r.get('platform')!='wechat':raise ValueError('identity platform needs explicit mapping')
                    user=ref('fa_user',r['user_id'])
                    for t,row in rows:
                        if t=='accounts' and row['id']==user:
                            if row.get('wx_openid') and row['wx_openid']!=r['openid']:raise ValueError('multiple WeChat openids on one account')
                            row['wx_openid']=r['openid']
                elif table=='fa_config':
                    name=r.get('name','');value=r.get('value') or ''
                    if name in ('discover_audit_mode','comment_audit_mode'):
                        if value not in ('manual','auto'):raise ValueError('unknown audit mode')
                        for t,row in rows:
                            if t=='schools':row['config']['post_audit_mode' if name=='discover_audit_mode' else name]=value
                    elif name.startswith('customer_service_'):
                        add('school_resources',dict(id=target,school_id=None,kind='service',title=name,content=value if 'qrcode' not in name else '',url=value if 'qrcode' in name else '',active=True),table,old)
                    else:warnings.append(dict(table=table,id=old,reason='non-business configuration excluded: '+name))
                elif table in ('fa_user','fa_admin'):
                    s=school(r) if r.get('school_id') else None
                    # All imported administrators require fresh local password provisioning; no global role inferred.
                    add('accounts',dict(id=target,username=f"legacy_{table[3:]}_{old}",display_name=r.get('nickname') or r.get('username') or f'用户{old}',active=r.get('status','normal')=='normal',bound_school_id=s,created_at=stamp(r.get('createtime'))),table,old)
                    if s:add('school_memberships',dict(school_id=s,account_id=target,role='member'))
                    if table=='fa_admin' or not any(str(t.get('user_id'))==str(old) and t.get('platform')=='wechat' for t in tables.get('fa_third',[])):warnings.append(dict(table=table,id=old,reason='password reset required; legacy hashes/tokens intentionally excluded'))
                elif table=='fa_admin_school':
                    if r.get('status','normal')=='normal':add('school_memberships',dict(school_id=school(r),account_id=ref('fa_admin',r['admin_id']),role='admin'))
                elif table=='fa_discover':
                    s=school(r);owner=ref('fa_user',r['user_id']);audit=r.get('audit_status')
                    if audit not in ('pending','approved','rejected'):raise ValueError('unknown audit_status')
                    visible=states.get(str(r.get('statusdata')))
                    if visible not in ('visible','hidden','deleted'):raise ValueError('statusdata requires explicit --states mapping')
                    images=media(str(r.get('coverimages') or r.get('coverimage') or '').split(','),s,owner)
                    add('posts',dict(id=target,school_id=s,author_id=owner,title=r.get('title') or '无标题',content=r.get('content') or r.get('description') or '',images=images,tags=[x for x in str(r.get('tag_ids') or '').split(',') if x],audit_status=audit,visibility=visible,top_sort=int(r.get('top_sort') or 0),browse=int(r.get('browse') or 0),created_at=stamp(r.get('createtime')),updated_at=stamp(r.get('updatetime') or r.get('createtime'))),table,old)
                elif table=='fa_discover_comment':
                    post=lookup['fa_discover'][str(r['discover_id'])];s=school(post)
                    audit=r.get('audit_status','pending')
                    if audit not in ('pending','approved','rejected'):raise ValueError('unknown comment audit_status')
                    parent=r.get('comment_id') or 0
                    parent_id=ref(table,parent) if str(parent)!='0' else None
                    if parent_id and str(lookup[table][str(parent)]['discover_id'])!=str(r['discover_id']):raise ValueError('cross-post comment parent')
                    add('comments',dict(id=target,school_id=s,post_id=ref('fa_discover',r['discover_id']),author_id=ref('fa_user',r['user_id']),parent_id=parent_id,content=r['content'],audit_status=audit,deleted=str(r.get('statusdata','1'))!='1',created_at=stamp(r.get('createtime'))),table,old)
                elif table in ('fa_discover_favor','fa_discover_collect'):
                    post=lookup['fa_discover'][str(r['discover_id'])];s=school(post);owner=ref('fa_user',r['user_id'])
                    if table=='fa_discover_favor' and str(r.get('typedata','1'))=='2':add('comment_reactions',dict(school_id=s,comment_id=ref('fa_discover_comment',r['comment_id']),account_id=owner))
                    else:add('post_reactions',dict(school_id=s,post_id=ref('fa_discover',r['discover_id']),account_id=owner,kind='collect' if table.endswith('collect') else 'like'))
                elif table=='fa_discover_attentions':
                    user=lookup['fa_user'][str(r['user_id'])];other=lookup['fa_user'][str(r['attention_id'])]
                    s=school(r,user)
                    if s!=school(other):raise ValueError('cross-school historical follow requires review')
                    add('follows',dict(school_id=s,follower_id=ref('fa_user',r['user_id']),target_id=ref('fa_user',r['attention_id'])))
                elif table=='fa_discover_log':
                    user=lookup['fa_user'][str(r['user_id'])];s=school(r,user);pid=r.get('discover_id')
                    add('notifications',dict(id=target,school_id=s,account_id=ref('fa_user',r['user_id']),kind=f"legacy_{r.get('typedata','unknown')}",object_id=ref('fa_discover',pid) if pid and str(pid)!='0' else None,read_at=stamp(r.get('updatetime') or r.get('createtime')) if str(r.get('readdata','0'))=='1' else None,created_at=stamp(r.get('createtime'))),table,old)
                elif table in ('fa_discover_report','fa_discover_feedback'):
                    user=lookup['fa_user'][str(r['user_id'])];s=school(r,user);state={'pending':'open','processing':'open','handled':'resolved','done':'resolved','rejected':'dismissed'}.get(r.get('status','pending'))
                    if not state:raise ValueError('unknown case status')
                    images=media(str(r.get('images') or '').split(','),s,ref('fa_user',r['user_id']))
                    add('cases',dict(id=target,school_id=s,account_id=ref('fa_user',r['user_id']),kind='report' if table.endswith('report') else 'feedback',post_id=ref('fa_discover',r['discover_id']) if table.endswith('report') else None,content=r.get('reason') or r.get('content') or '',contact=r.get('contact') or '',state=state,images=images,created_at=stamp(r.get('createtime'))),table,old)
                elif table=='fa_wecom_send_log':
                    add('historical_delivery_events',dict(id=target,school_id=school(r),post_id=ref('fa_discover',r['discover_id']),source_id=str(old),evidence={k:r[k] for k in ['chat_hash','batch_key','send_type'] if k in r},created_at=stamp(r.get('createtime'))),table,old)
                else:
                    kind={'fa_home_banner':'banner','fa_school_group_qrcode':'qrcode','fa_discover_tag':'tag','fa_discover_blocked_word':'blocked_word'}[table]
                    s=school(r) if str(r.get('school_id') or '0')!='0' else None
                    add('school_resources',dict(id=target,school_id=s,kind=kind,title=r.get('title') or r.get('name') or r.get('word') or f'资源{old}',content=r.get('content') or r.get('note') or '',url=r.get('image') or r.get('qrcode') or r.get('url') or '',active=r.get('status','normal')=='normal',sort_order=int(r.get('weigh') or 0)),table,old)
            except (ValueError,KeyError,OSError,OverflowError) as e:
                errors.append(dict(table=table,id=old,reason=str(e)))
    for legacy_school in tables.get('fa_school',[]):
        chats=str(legacy_school.get('wecom_chat_ids') or '').replace('\n',',').split(',')
        for chat in filter(None,(c.strip() for c in chats)):
            if not corp_id:errors.append(dict(table='fa_school',id=legacy_school['id'],reason='wecom_chat_ids requires --corp-id'));continue
            gid=identity('wecom_group',corp_id+':'+chat)
            try:add('wecom_groups',dict(id=gid,school_id=ref('fa_school',legacy_school['id']),corp_id=corp_id,chat_id=chat,name=legacy_school['name']+'客户群'))
            except ValueError as e:errors.append(dict(table='fa_school',id=legacy_school['id'],reason=str(e)))
        if legacy_school.get('wecom_manager_userids'):warnings.append(dict(table='fa_school',id=legacy_school['id'],reason='manager userids require explicit account binding; no authorization inferred'))
    for table in tables:
        if table not in SUPPORTED and tables[table]:errors.append(dict(table=table,id=None,reason='unmapped source table; explicit mapping/archival decision required'))
    prior_rows={}; previous_hash=None
    def row_key(table,row):
        keys={'school_memberships':['school_id','account_id'],'post_reactions':['post_id','account_id','kind'],'comment_reactions':['comment_id','account_id'],'follows':['school_id','follower_id','target_id']}.get(table,['id'])
        return table,tuple((key,row.get(key)) for key in keys)
    if previous:
        previous_hash=hashlib.sha256(previous.read_bytes()).hexdigest()
        with tempfile.TemporaryDirectory(prefix='wekeeper-previous-') as directory:
            prior=convert(previous,Path(directory),states,media_root,media_map,corp_id)
            if not prior['ready_to_apply']:errors.append(dict(table='snapshot',id=None,reason='previous snapshot has unresolved conversion errors'))
            else:prior_rows={row_key(t,r):r for t,r in json.loads((Path(directory)/'normalized.json').read_text())}
        current_keys={row_key(t,r) for t,r in rows}
        if set(prior_rows)-current_keys:errors.append(dict(table='snapshot',id=None,reason='hard deletions detected; use explicit reviewed deletion migration'))
    out.mkdir(parents=True,exist_ok=True)
    os.chmod(out,0o700)
    report=dict(previous_hash=previous_hash,source_hash=digest,batch_id=batch,counts=counts,rejected=errors,warnings=warnings,source_counts={k:len(v) for k,v in tables.items()},ready_to_apply=not errors)
    (out/'report.json').write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n')
    (out/'mapping.json').write_text(json.dumps(mappings,ensure_ascii=False,indent=2)+'\n')
    (out/'normalized.json').write_text(json.dumps(rows,ensure_ascii=False,indent=2)+'\n')
    apply=out/'apply.sql'
    if errors:
        if apply.exists():apply.unlink()
        return report
    sql=["\\set ON_ERROR_STOP on",'BEGIN;',"SET LOCAL standard_conforming_strings=on;", "SELECT pg_advisory_xact_lock(hashtext('wekeeper-legacy-import'));",f"SELECT NOT EXISTS(SELECT 1 FROM migration_runs WHERE source_hash={literal(digest)}) AS apply_batch \\gset",'\\if :apply_batch']
    sql.append('CREATE TEMP TABLE migration_assertion(ok BOOLEAN NOT NULL CHECK(ok)) ON COMMIT DROP;')
    if previous_hash:sql.append(f"INSERT INTO migration_assertion SELECT EXISTS(SELECT 1 FROM migration_runs WHERE source_hash={literal(previous_hash)});")
    for table,row in rows:
        prior=prior_rows.get(row_key(table,row))
        if prior==row:continue
        if prior is not None:
            # Compare all imported columns. Any new-system write causes a blocking conflict.
            conditions=' AND '.join(f"{k} IS NOT DISTINCT FROM {literal(v)}" for k,v in prior.items())
            updates=','.join(f"{k}={literal(v)}" for k,v in row.items())
            sql.append(f"WITH changed AS (UPDATE {table} SET {updates} WHERE {conditions} RETURNING 1) INSERT INTO migration_assertion SELECT count(*)=1 FROM changed;")
            continue
        values=[]
        for value in row.values():
            if isinstance(value,dict) and '__bytes' in value:values.append(f"decode({literal(value['__bytes'])},'base64')")
            else:values.append(literal(value))
        sql.append(f"INSERT INTO {table} ({','.join(row)}) VALUES ({','.join(values)});")
    for table,old,target in mappings:
        sql.append(f"INSERT INTO legacy_id_map VALUES({literal(table)},{literal(old)},{literal(target)},{literal(batch)}) ON CONFLICT(source_table,legacy_id) DO NOTHING;")
    sql.append(f"INSERT INTO migration_runs(id,source_hash,summary) VALUES({literal(batch)},{literal(digest)},{literal({'counts':counts})});")
    sql+=['\\endif','COMMIT;']
    apply.write_text('\n'.join(sql)+'\n');os.chmod(apply,0o600)
    return report

if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('source',type=Path);parser.add_argument('--out',type=Path,required=True)
    parser.add_argument('--states',type=Path,help='JSON map of legacy statusdata to visible/hidden/deleted; default accepts only 1')
    parser.add_argument('--previous',type=Path,help='previous full snapshot for guarded incremental replay');parser.add_argument('--corp-id');parser.add_argument('--media-root',type=Path);parser.add_argument('--media-map',type=Path)
    args=parser.parse_args()
    result=convert(args.source,args.out,json.loads(args.states.read_text()) if args.states else {'1':'visible'},args.media_root,json.loads(args.media_map.read_text()) if args.media_map else None,args.corp_id,args.previous)
    print(json.dumps({'ready_to_apply':result['ready_to_apply'],'counts':result['counts'],'rejected':len(result['rejected']),'report':str(args.out/'report.json')},ensure_ascii=False))
    raise SystemExit(0 if result['ready_to_apply'] else 2)
