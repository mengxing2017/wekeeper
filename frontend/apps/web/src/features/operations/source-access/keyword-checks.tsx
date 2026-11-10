import {useState} from 'react'
import {useQuery} from '@tanstack/react-query'
import {Alert,Button,Descriptions,Space,Table,Typography} from 'antd'
import {keywordRules,type KeywordCheck} from './api'
const stages={original_common:'原文公共范围',original_workspace:'原文工作空间',final:'最终正文'}
export function KeywordChecks({route,receipt}:{route:string;receipt:string}){
 const [page,setPage]=useState(1),[check,setCheck]=useState<KeywordCheck>(),[hitPage,setHitPage]=useState(1)
 const checks=useQuery({queryKey:['keyword-checks',route,receipt,page],queryFn:()=>keywordRules.checks(route,receipt,page),refetchInterval:10000})
 const hits=useQuery({queryKey:['keyword-hits',route,receipt,check?.id,hitPage],queryFn:()=>keywordRules.hits(route,receipt,check!.id,hitPage),enabled:!!check})
 return <Space orientation="vertical" style={{width:'100%'}}>
  <Typography.Title level={5}>敏感词检查证据</Typography.Title>
  {(checks.error||hits.error)&&<Alert type="error" title={checks.error?.message??hits.error?.message}/>}
  <Table<KeywordCheck> rowKey="id" dataSource={checks.data?.items??[]} pagination={false} columns={[{title:'阶段',render:(_,r)=>stages[r.stage]},{title:'结论',render:(_,r)=>r.outcome==='passed'?'通过':'拦截'},{title:'命中总数',dataIndex:'total_hits'},{title:'时间',dataIndex:'created_at'},{title:'操作',render:(_,r)=><Button onClick={()=>{setCheck(r);setHitPage(1)}}>查看证据</Button>}]}/>
  <Space><Button disabled={page===1} onClick={()=>setPage(page-1)}>上一页</Button>{page}<Button disabled={!checks.data?.has_more} onClick={()=>setPage(page+1)}>下一页</Button></Space>
  {check&&<>
   <Descriptions column={1} items={[{key:'algorithm',label:'规范化算法',children:`${check.algorithm} / Unicode ${check.unicode_version}`},{key:'fingerprint',label:'有效上下文指纹',children:check.effective_fingerprint},{key:'input',label:'输入摘要',children:check.input_digest}]}/>
   <Table rowKey="scope_id" dataSource={check.context.versions} pagination={false} columns={[{title:'适用范围',dataIndex:'scope_id'},{title:'修订',dataIndex:'revision'},{title:'版本',dataIndex:'version_id'}]}/>
   <Typography.Text>命中规则（完整列表分页）</Typography.Text>
   <Table rowKey="id" dataSource={hits.data?.items??[]} pagination={false} columns={[{title:'规则标识',dataIndex:'id'},{title:'范围',dataIndex:'scope_id'},{title:'敏感词',dataIndex:'word'},{title:'规则版本',dataIndex:'version_id'}]}/>
   <Space><Button disabled={hitPage===1} onClick={()=>setHitPage(hitPage-1)}>上一页规则</Button>{hitPage}<Button disabled={!hits.data?.has_more} onClick={()=>setHitPage(hitPage+1)}>下一页规则</Button></Space>
   <Typography.Text>位置为规范化后文本的 Unicode 标量区间，左闭右开。样例最多 100 条{hits.data?.truncated?'，已截断；上方完整命中规则不受样例上限影响':''}。</Typography.Text>
   <Table rowKey={r=>`${r.rule_id}:${r.field}:${r.start}:${r.end}`} dataSource={hits.data?.samples??[]} pagination={{pageSize:10}} columns={[{title:'规则标识',dataIndex:'rule_id'},{title:'字段',dataIndex:'field'},{title:'起点',dataIndex:'start'},{title:'终点',dataIndex:'end'}]}/>
  </>}
 </Space>
}
