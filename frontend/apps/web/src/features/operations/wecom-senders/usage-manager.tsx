import {useState} from 'react'
import {useQuery} from '@tanstack/react-query'
import {Alert,Button,Card,Select,Space} from 'antd'
import {product,type UsageSummary,type BusinessPreview} from './api'
export function OfficialUsageManager(){
 const[member,setMember]=useState(''),[account,setAccount]=useState(''),[groups,setGroups]=useState<string[]>([]),[preview,setPreview]=useState<BusinessPreview<UsageSummary>>(),[error,setError]=useState(''),[message,setMessage]=useState(''),[busy,setBusy]=useState(false)
 const q=useQuery({queryKey:['official-usage-options'],queryFn:()=>product.officialUsage.options()})
 const selected=q.data?.members.find(v=>v.id===member),targets=q.data?.groups.filter(v=>v.owner_member_id===member)??[]
 async function run(action:()=>Promise<unknown>){if(busy)return;setBusy(true);setError('');try{await action()}catch(e){setError(e instanceof Error?e.message:'办理失败')}finally{setBusy(false)}}
 return <Card title="配置官方成员使用范围">{(error||q.error)&&<Alert type="error" title={error||q.error?.message}/>} {message&&<Alert type="success" title={message}/>}
 {!preview?<Space orientation="vertical">
 {<><Select aria-label="官方成员" placeholder="选择已核实成员" showSearch optionFilterProp="label" value={member||undefined} options={q.data?.members.filter(v=>v.verified).map(v=>({value:v.id,label:`${v.name} · ${v.account_name??'尚未关联账号'}`}))} onChange={m=>{setMember(m);const id=q.data?.members.find(v=>v.id===m)?.bound_account_id??'';setAccount(id);setGroups([])}}/>
 <Select aria-label="使用账号" placeholder="选择已有登录账号" value={account||undefined} options={q.data?.accounts.filter(v=>!v.member_id||v.member_id===member).map(v=>({value:v.id,label:v.name}))} onChange={id=>{setAccount(id)}}/>
 <Select aria-label="使用客户群" mode="multiple" placeholder="选择当前成员担任群主的群" value={groups} options={targets.map(v=>({value:v.id,label:v.name??'客户群'}))} onChange={setGroups}/>
 <p>{q.data?.reason??(!targets.length?'当前没有允许关联的群。目录、身份或群主条件需由平台负责人核对；不会为取得候选自动提升权限。':`可关联 ${targets.length} 个群；范围及本人群主条件在确认时重验。`)}</p>
 <Button loading={busy} disabled={!member||!account||!groups.length||!q.data?.generation} onClick={()=>void run(async()=>setPreview(await product.officialUsage.preview({member_id:member,account_id:account,generation:q.data!.generation!,group_ids:groups})))}>核对使用范围</Button></>}
 </Space>:<><p>成员：{selected?.name} · 账号：{q.data?.accounts.find(v=>v.id===account)?.name}</p><ul>{groups.map(id=><li key={id}>{targets.find(v=>v.id===id)?.name??'客户群'}</li>)}</ul><p>{preview.summary.effects.bind_identity?'本次将关联账号与成员身份。':'身份关系保持不变。'}{preview.summary.effects.enable_qualification?'本次将启用官方发送资格。':'发送资格保持启用。'}{preview.summary.effects.restore_group_ids.length>0&&`本次将恢复 ${preview.summary.effects.restore_group_ids.length} 个已停用群。`}确认后办理上述使用范围。</p><Button onClick={()=>setPreview(undefined)}>返回修改</Button><Button type="primary" loading={busy} onClick={()=>void run(async()=>{await product.officialUsage.confirm({preview_id:preview.id,digest:preview.digest});setMessage('范围已办理；实际发送仍核验渠道、目录和成员确认条件');setPreview(undefined);await q.refetch()})}>确认授权</Button></>}
 </Card>
}
