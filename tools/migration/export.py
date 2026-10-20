#!/usr/bin/env python3
"""Read-only consistent snapshot from MySQL. Requires pymysql; never writes to MySQL."""
import argparse
import json
import os
from pathlib import Path
from datetime import date,datetime
from decimal import Decimal
from convert import SUPPORTED

def encode(v):
    if isinstance(v,(date,datetime)):return v.isoformat()
    if isinstance(v,Decimal):return str(v)
    if isinstance(v,bytes):return v.decode('utf-8')
    raise TypeError(type(v).__name__)

if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--config',type=Path,required=True,help='0600 JSON {host,port,user,password,database}');parser.add_argument('--out',type=Path,required=True);parser.add_argument('--legal-docs',type=Path);args=parser.parse_args()
    if args.config.stat().st_mode&0o077:raise SystemExit('MySQL configuration must have mode 0600')
    import pymysql
    config=json.loads(args.config.read_text());conn=pymysql.connect(**config,charset='utf8mb4',cursorclass=pymysql.cursors.DictCursor)
    try:
        with conn.cursor() as cur:
            cur.execute('SET TRANSACTION READ ONLY');cur.execute('START TRANSACTION WITH CONSISTENT SNAPSHOT');cur.execute('SHOW TABLES');available={next(iter(r.values())) for r in cur.fetchall()};tables={}
            for table in SUPPORTED:
                if table not in available or table=='fa_legal_docs':continue
                cur.execute(f'SELECT * FROM `{table}` ORDER BY id');rows=cur.fetchall()
                if table=='fa_config':rows=[r for r in rows if r.get('name','').startswith('customer_service_') or r.get('name') in ('discover_audit_mode','comment_audit_mode')]
                if table=='fa_third':rows=[{k:r.get(k) for k in ('id','user_id','platform','openid')} for r in rows]
                for row in rows:
                    for key in ('password','salt','token','secret','session_key'):row.pop(key,None)
                tables[table]=rows
            if args.legal_docs:
                legal=json.loads(args.legal_docs.read_text());tables['fa_legal_docs']=[{'id':key,'title':v['title'],'content':json.dumps(v,ensure_ascii=False)} for key,v in legal['docs'].items()]
            with os.fdopen(os.open(args.out,os.O_WRONLY|os.O_CREAT|os.O_EXCL,0o600),'w') as out:json.dump({'tables':tables,'source_tables':sorted(available),'excluded_tables':sorted(available-set(SUPPORTED))},out,ensure_ascii=False,default=encode)
    finally:conn.rollback();conn.close()
    print(f'Snapshot written: {args.out}; review excluded_tables before cutover.')
