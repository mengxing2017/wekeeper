import {useState} from 'react'
import {useQuery} from '@tanstack/react-query'
import {Alert,Button,Card,Select,Space} from 'antd'
import {product,workspace,type UsageInput,type BusinessPreview} from './api'
export function OfficialUsageManager({school:initialSchool,global}:{school?:string;global:boolean}){
 const[chosenSchool,setSchool]=useState(initialSchool??''),[member,setMember]=useState(''),[account,setAccount]=useState(''),[role,setRole]=useState<'operator'|'admin'>('operator'),[groups,setGroups]=useState<string[]>([]),[preview,setPreview]=useState<BusinessPreview<UsageInput>>(),[error,setError]=useState(''),[message,setMessage]=useState(''),[busy,setBusy]=useState(false)
 const school=global?chosenSchool:initialSchool??''
 const schools=useQuery({queryKey:['schools'],queryFn:workspace.schools,enabled:global})
 const q=useQuery({queryKey:['official-usage-options',school],queryFn:()=>product.officialUsage.options(school),enabled:!!school})
 const selected=q.data?.members.find(v=>v.id===member),targets=q.data?.groups.filter(v=>v.owner_member_id===member)??[]
 async function run(action:()=>Promise<unknown>){if(busy)return;setBusy(true);setError('');try{await action()}catch(e){setError(e instanceof Error?e.message:'办理失败')}finally{setBusy(false)}}
 return <Card title="配置学校官方成员使用范围">{(error||q.error)&&<Alert type="error" title={error||q.error?.message}/>} {message&&<Alert type="success" title={message}/>}
 {!preview?<Space orientation="vertical">
 {global&&<Select aria-label="办理学校" placeholder="选择学校" value={school||undefined} options={schools.data?.filter(s=>s.active).map(s=>({value:s.id,label:s.name}))} onChange={s=>{setSchool(s);setMember('');setAccount('');setGroups([])}}/>}
 {school&&<><Select aria-label="官方成员" placeholder="选择已核实成员" showSearch optionFilterProp="label" value={member||undefined} options={q.data?.members.filter(v=>v.verified).map(v=>({value:v.id,label:`${v.name} · ${v.account_name??'尚未关联账号'}`}))} onChange={m=>{setMember(m);const id=q.data?.members.find(v=>v.id===m)?.bound_account_id??'';setAccount(id);setRole(q.data?.accounts.find(v=>v.id===id)?.role==='admin'?'admin':'operator');setGroups([])}}/>
 <Select aria-label="使用账号" placeholder="选择已有登录账号" value={account||undefined} options={q.data?.accounts.filter(v=>!v.member_id||v.member_id===member).map(v=>({value:v.id,label:v.name}))} onChange={id=>{setAccount(id);setRole(q.data?.accounts.find(v=>v.id===id)?.role==='admin'?'admin':'operator')}}/>
 <Select aria-label="学校职责" value={role} options={[{value:'operator',label:'学校运营'},{value:'admin',label:'学校管理员'}]} onChange={setRole}/>
 <Select aria-label="使用客户群" mode="multiple" placeholder="选择当前成员担任群主的群" value={groups} options={targets.map(v=>({value:v.id,label:v.name??'客户群'}))} onChange={setGroups}/>
 <p>{q.data?.reason??(!targets.length?'当前没有允许关联的群。目录、身份或群主条件需由平台负责人核对；不会为取得候选自动提升权限。':`可关联 ${targets.length} 个群；学校范围及本人群主条件在确认时重验。`)}</p>
 <Button loading={busy} disabled={!member||!account||!groups.length||!q.data?.generation} onClick={()=>void run(async()=>setPreview(await product.officialUsage.preview(school,{member_id:member,account_id:account,generation:q.data!.generation!,role,group_ids:groups})))}>核对使用范围</Button></>}
 </Space>:<><p>学校：{schools.data?.find(s=>s.id===school)?.name??'当前学校'} · 成员：{selected?.name} · 账号：{q.data?.accounts.find(v=>v.id===account)?.name} · 职责：{role==='admin'?'学校管理员':'学校运营'}</p><ul>{groups.map(id=><li key={id}>{targets.find(v=>v.id===id)?.name??'客户群'}</li>)}</ul><p>{preview.requires_approval?'身份或职责变化需要平台审批；提交后不会提前授予权限。':'确认后办理已核对的使用范围。'}</p><Button onClick={()=>setPreview(undefined)}>返回修改</Button><Button type="primary" loading={busy} onClick={()=>void run(async()=>{const v=await product.officialUsage.confirm(school,{preview_id:preview.id,digest:preview.digest});setMessage(v.state==='approved'?'范围已办理；实际发送仍核验渠道、目录和成员确认条件':'申请已提交，等待平台审批');setPreview(undefined);await q.refetch()})}>{preview.requires_approval?'确认提交申请':'确认授权'}</Button></>}
 </Card>
}
