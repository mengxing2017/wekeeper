import json
import tempfile
import unittest
from pathlib import Path
from convert import convert

class Conversion(unittest.TestCase):
    def snapshot(self):return {'tables':{'fa_school':[{'id':1,'name':'Test'}],'fa_user':[{'id':2,'nickname':'Member','school_id':1}],'fa_discover':[{'id':3,'school_id':1,'user_id':2,'title':"Quote's test",'content':'文本','statusdata':'1','audit_status':'approved'}]}}
    def test_deterministic_and_blocks_unknown_states(self):
        with tempfile.TemporaryDirectory() as d:
            p=Path(d);source=p/'source.json';source.write_text(json.dumps(self.snapshot()));a=convert(source,p/'one',{'1':'visible'});b=convert(source,p/'two',{'1':'visible'})
            self.assertTrue(a['ready_to_apply']);self.assertEqual(a['batch_id'],b['batch_id']);self.assertEqual((p/'one/apply.sql').read_text(),(p/'two/apply.sql').read_text());self.assertIn("Quote''s test",(p/'one/apply.sql').read_text())
            doc=self.snapshot();doc['tables']['fa_discover'][0]['statusdata']='3';source.write_text(json.dumps(doc));r=convert(source,p/'one',{'1':'visible'});self.assertFalse(r['ready_to_apply']);self.assertFalse((p/'one/apply.sql').exists())
    def test_orphan_and_unknown_table_block_application(self):
        with tempfile.TemporaryDirectory() as d:
            p=Path(d);source=p/'source.json';doc=self.snapshot();doc['tables']['fa_discover'][0]['user_id']=999;doc['tables']['unknown']=[{'id':1}];source.write_text(json.dumps(doc));r=convert(source,p/'out',{'1':'visible'});self.assertEqual(len(r['rejected']),2)
if __name__=='__main__':unittest.main()
