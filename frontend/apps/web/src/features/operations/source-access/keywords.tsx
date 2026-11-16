import { TableRowActions } from '../../../shared/table-row-actions'
import { TableText } from '../../../shared/table-text'
import {useState} from 'react'
import {useQuery,useQueryClient} from '@tanstack/react-query'
import {Alert,Button,Input,Modal,Select,Space,Switch,Table,Typography} from 'antd'
import {keywordRules,type KeywordEntry} from './api'
import {parseKeywordCsv,type ImportRow} from './keyword-import'

export function KeywordRules(){
 const cache=useQueryClient()
 const words=useQuery({queryKey:['sensitive-words'],queryFn:keywordRules.list})
 const [search,setSearch]=useState(''),[status,setStatus]=useState('all'),[error,setError]=useState(''),[busy,setBusy]=useState(false)
 const [editing,setEditing]=useState<{index:number|null;word:string;enabled:boolean;filter_enabled:boolean;revision:number;base:KeywordEntry[]}|null>(null)
 const [importRows,setImportRows]=useState<ImportRow[]|null>(null)
 const [importSnapshot,setImportSnapshot]=useState<{revision:number;filter_enabled:boolean;entries:KeywordEntry[]}>()
 const entries=words.data?.entries??[]
 const visible=entries.filter(x=>x.word.normalize('NFKC').toLowerCase().includes(search.normalize('NFKC').toLowerCase())&&(status==='all'||x.enabled===(status==='enabled')))
 const save=async(next:KeywordEntry[],revision:number,filterEnabled:boolean)=>{
  setBusy(true);setError('')
  try{await keywordRules.save({expected_revision:revision,filter_enabled:filterEnabled,entries:next.map(({word,enabled})=>({word,enabled}))});await cache.invalidateQueries({queryKey:['sensitive-words']});return true}
  catch(e){setError((e as Error).message);return false}finally{setBusy(false)}
 }
 const commitEdit=async()=>{
  if(!editing||!words.data)return
  if(!editing.word.trim()){setError('请输入敏感词');return}
  const next=editing.base.map(({word,enabled})=>({word,enabled}))
  const item={word:editing.word,enabled:editing.enabled}
  if(editing.index===null)next.push(item);else next[editing.index]=item
  if(await save(next,editing.revision,editing.filter_enabled))setEditing(null)
 }
 const readFile=async(file:File)=>{
  setImportRows(null);setError('')
  try{const snapshot=words.data;if(!snapshot)throw new Error('请等待敏感词列表加载');if(file.size>1024*1024)throw new Error('文件不能超过 1 MB');const text=new TextDecoder('utf-8',{fatal:true}).decode(await file.arrayBuffer());setImportRows(parseKeywordCsv(text,snapshot.entries));setImportSnapshot({revision:snapshot.revision,filter_enabled:snapshot.filter_enabled,entries:snapshot.entries.map(({word,enabled})=>({word,enabled}))})}catch(e){setError(e instanceof Error?e.message:'文件读取失败')}
 }
 return <Space orientation="vertical" style={{width:'100%'}}>
  <Typography.Title level={5}>全局拦截设置</Typography.Title>
  <Space><Typography.Text>全局启用敏感词拦截</Typography.Text><Switch aria-label="全局启用敏感词拦截" loading={words.isLoading||busy} disabled={!words.data||!!words.error||busy||words.isFetching} checked={words.data?.filter_enabled??false} onChange={enabled=>{if(words.data)void save(entries,words.data.revision,enabled)}}/><Typography.Text>{!words.data||words.error?'状态未加载':words.data.filter_enabled?'全局已启用':'全局已停用'}</Typography.Text></Space>
  <Alert type="info" title="默认停用。启用后，对所有来源消息执行敏感词检查；停用后，所有来源消息跳过敏感词检查。切换不修改词库，已拦截的消息不会恢复发送。"/>
  {(words.error||error)&&<Alert type="error" title={error||words.error?.message}/>}
  <Space wrap><Input aria-label="搜索敏感词" placeholder="搜索敏感词" value={search} onChange={e=>setSearch(e.target.value)}/><Select aria-label="状态筛选" style={{width:128}} value={status} onChange={setStatus} options={[{value:'all',label:'全部状态'},{value:'enabled',label:'启用'},{value:'disabled',label:'停用'}]}/><Button type="primary" disabled={!words.data||busy||entries.length>=2000} onClick={()=>{setError('');if(words.data)setEditing({index:null,word:'',enabled:true,revision:words.data.revision,filter_enabled:words.data.filter_enabled,base:entries})}}>添加</Button><Button disabled={busy} onClick={()=>words.refetch()}>刷新</Button></Space>
  <Table<KeywordEntry> tableLayout="fixed" scroll={{x:440}} rowKey={r=>String(entries.indexOf(r))} loading={words.isLoading} dataSource={visible} pagination={{pageSize:20}} columns={[
   {title:'敏感词',dataIndex:'word', render:value=><TableText value={value}/>},
   {title:'启用', width:88,render:(_,r)=><Switch aria-label={`${r.word}启用状态`} disabled={busy} checked={r.enabled} onChange={enabled=>{if(words.data)void save(entries.map((x,i)=>i===entries.indexOf(r)?{word:x.word,enabled}:{word:x.word,enabled:x.enabled}),words.data.revision,words.data.filter_enabled)}}/>},
   {title:'操作', width:128,render:(_,r)=><TableRowActions rowLabel={r.word} actions={[
     {key:'edit',label:'编辑',disabled:busy,onClick:()=>{setError('');if(words.data)setEditing({index:entries.indexOf(r),word:r.word,enabled:r.enabled,revision:words.data.revision,filter_enabled:words.data.filter_enabled,base:entries})}},
     {key:'delete',label:'删除',disabled:busy,confirm:{title:'删除此敏感词？',okText:'确认删除'},onClick:()=>words.data&&save(entries.filter(x=>x!==r),words.data.revision,words.data.filter_enabled)},
    ]}/>},
  ]}/>
  <Typography.Text>批量导入 UTF-8 CSV，表头为 word，可选 enabled（true/false）。最多 2000 条。</Typography.Text>
  <input aria-label="导入敏感词 CSV" type="file" accept=".csv,text/csv" disabled={!words.data||busy} onChange={e=>{const file=e.target.files?.[0];if(file)void readFile(file);e.target.value=''}}/>
  {importRows&&<><Table tableLayout="fixed" scroll={{x:660}} rowKey={r=>String(r.line)} dataSource={importRows} pagination={{pageSize:20}} columns={[{title:'行号', width:80,dataIndex:'line'},{title:'敏感词',dataIndex:'word', render:value=><TableText value={value}/>},{title:'状态', width:80,render:(_,r)=>r.enabled?'启用':'停用'},{title:'校验',render:(_,r)=><TableText value={r.error||'通过'}/>},{title:'操作', width:128,render:(_,r)=><Button onClick={()=>setImportRows(importRows.filter(x=>x!==r))}>移除</Button>}]}/><Space><Button loading={busy} disabled={!importRows.length||importRows.some(r=>!!r.error)||!importSnapshot} onClick={async()=>{if(importSnapshot&&await save([...importSnapshot.entries,...importRows.map(({word,enabled})=>({word,enabled}))],importSnapshot.revision,importSnapshot.filter_enabled))setImportRows(null)}}>确认导入</Button><Button disabled={busy} onClick={()=>setImportRows(null)}>取消</Button></Space></>}
  <Modal open={editing!==null} title={editing?.index===null?'添加敏感词':'编辑敏感词'} okText="保存" cancelText="取消" confirmLoading={busy} onCancel={()=>setEditing(null)} onOk={()=>void commitEdit()} destroyOnHidden>
   {error&&<Alert type="error" title={error}/>}
   <Input aria-label="敏感词" value={editing?.word??''} onChange={e=>setEditing(editing?{...editing,word:e.target.value}:null)} maxLength={512}/>
  </Modal>
 </Space>
}
