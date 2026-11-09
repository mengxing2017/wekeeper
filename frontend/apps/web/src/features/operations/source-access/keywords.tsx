import {useState} from 'react'
import {useQuery,useQueryClient} from '@tanstack/react-query'
import {Alert,Button,Drawer,Input,Space,Switch,Table,Typography} from 'antd'
import {keywordRules,type KeywordScope,type KeywordEntry,type KeywordVersion} from './api'
const kinds:Record<string,string>={global:'全局',source:'调用方',unit:'业务单位',route:'路由',workspace:'工作空间'}
export function KeywordRules(){
 const scopes=useQuery({queryKey:['keyword-scopes'],queryFn:keywordRules.scopes,refetchInterval:10000})
 const [selected,setSelected]=useState<KeywordScope>()
 return <Space orientation="vertical" style={{width:'100%'}}>
  <Alert type="info" title="各适用范围的规则叠加生效。发布后自动重检受影响内容；已发送和结果未知的事实保留。"/>
  {scopes.error&&<Alert type="error" title={scopes.error.message}/>}
  <Button onClick={()=>scopes.refetch()}>刷新规则范围</Button>
  <Table<KeywordScope> rowKey="id" loading={scopes.isFetching} dataSource={scopes.data?.items??[]} pagination={{pageSize:30}} columns={[
   {title:'范围',render:(_,r)=>kinds[r.kind]},{title:'名称',dataIndex:'label'},{title:'通道',render:(_,r)=>r.channel==='mobile'?'手机':r.channel==='wecom'?'企业微信':'—'},
   {title:'当前修订',dataIndex:'revision'},{title:'重检扫描',render:(_,r)=>r.recheck_complete?'扫描完成':'扫描中'},
   {title:'操作',render:(_,r)=><Button onClick={()=>setSelected(r)}>查看与发布</Button>},
  ]}/>
  <Drawer open={!!selected} title="关键字规则与版本历史" size="large" onClose={()=>setSelected(undefined)} destroyOnHidden>
   {selected&&<RuleEditor key={selected.id} scope={selected}/>}
  </Drawer>
 </Space>
}
function RuleEditor({scope}:{scope:KeywordScope}){
 const cache=useQueryClient(),[page,setPage]=useState(1),[entries,setEntries]=useState<KeywordEntry[]|null>(null),[revision,setRevision]=useState(scope.revision),[reason,setReason]=useState(''),[error,setError]=useState(''),[busy,setBusy]=useState(false)
 const versions=useQuery({queryKey:['keyword-versions',scope.id,page],queryFn:()=>keywordRules.versions(scope.id,page)})
 const [historical,setHistorical]=useState<KeywordVersion>()
 const start=()=>{const current=versions.data?.items[0];if(page!==1||!current)return;setEntries(current.entries.map(({word,enabled})=>({word,enabled})));setRevision(current.revision);setReason('');setError('')}
 const publish=async()=>{if(!reason.trim()){setError('请填写发布原因');return}setBusy(true);setError('');try{await keywordRules.publish(scope.id,{expected_revision:revision,reason,entries:entries??[]});setEntries(null);await Promise.all([versions.refetch(),cache.invalidateQueries({queryKey:['keyword-scopes']})])}catch(e){setError((e as Error).message)}finally{setBusy(false)}}
 return <Space orientation="vertical" style={{width:'100%'}}>
  <Typography.Text>{kinds[scope.kind]}：{scope.label}</Typography.Text>
  {versions.error&&<Alert type="error" title={versions.error.message}/>}{error&&<Alert type="error" title={error}/>}
  {entries===null?<Button disabled={!scope.can_edit||page!==1||!versions.data?.items.length} onClick={start}>编辑当前规则</Button>:<>
   <Alert type="info" title="每个范围最多 2000 条。相同规范化关键字不能重复；空规则集需明确发布。关闭条目后保留版本记录。"/>
   <Table rowKey={r=>String(entries.indexOf(r))} dataSource={entries} pagination={{pageSize:20}} columns={[
    {title:'关键字',render:(_,r,i)=><Input aria-label={`关键字 ${i+1}`} value={r.word} onChange={e=>setEntries(entries.map((x,j)=>j===entries.indexOf(r)?{...x,word:e.target.value}:x))}/>},
    {title:'启用',render:(_,r)=><Switch checked={r.enabled} onChange={enabled=>setEntries(entries.map((x,j)=>j===entries.indexOf(r)?{...x,enabled}:x))}/>},
    {title:'操作',render:(_,r)=><Button onClick={()=>setEntries(entries.filter((_,j)=>j!==entries.indexOf(r)))}>移除</Button>},
   ]}/>
   <Button disabled={entries.length>=2000} onClick={()=>setEntries([...entries,{word:'',enabled:true}])}>添加关键字</Button>
   <Input.TextArea aria-label="发布原因" placeholder="发布原因" maxLength={2000} value={reason} onChange={e=>setReason(e.target.value)}/>
   <Space><Button type="primary" loading={busy} onClick={publish}>发布新版本</Button><Button disabled={busy} onClick={()=>setEntries(null)}>取消编辑</Button></Space>
  </>}
  <Typography.Title level={5}>发布历史</Typography.Title>
  <Table<KeywordVersion> rowKey="id" dataSource={versions.data?.items??[]} loading={versions.isFetching} pagination={false} columns={[{title:'修订',dataIndex:'revision'},{title:'原因',dataIndex:'reason'},{title:'条目数',render:(_,r)=>r.entries.length},{title:'时间',dataIndex:'created_at'},{title:'操作',render:(_,r)=><Button onClick={()=>setHistorical(r)}>查看条目</Button>}]}/>
  <Space><Button disabled={page===1||entries!==null} onClick={()=>setPage(page-1)}>上一页</Button><Typography.Text>{page}</Typography.Text><Button disabled={!versions.data?.has_more||entries!==null} onClick={()=>setPage(page+1)}>下一页</Button></Space>
  {historical&&<><Typography.Title level={5}>修订 {historical.revision} 的条目</Typography.Title><Table rowKey={r=>String(historical.entries.indexOf(r))} dataSource={historical.entries} pagination={{pageSize:20}} columns={[{title:'原始关键字',dataIndex:'word'},{title:'规范化文本',dataIndex:'normalized'},{title:'状态',render:(_,r)=>r.enabled?'启用':'停用'}]}/></>}
 </Space>
}
