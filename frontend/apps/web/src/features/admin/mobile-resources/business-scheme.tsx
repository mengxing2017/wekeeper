import {useRef,useState} from 'react'
import {useMutation,useQuery,useQueryClient} from '@tanstack/react-query'
import {Alert,Button,Card,Checkbox,Drawer,Input,InputNumber,Select,Space,Switch} from 'antd'
import {sharedSend,type BusinessScheme,type BusinessSchemeInput} from './api'
import {createIdempotencyKey} from '../../../shared/idempotency-key'
import {newPolicyWindow,policyClock,policyMinute,policyWeekdays} from './shared-policy-time'
const limit={interval_seconds:30,window_seconds:3600,max_calls:30,daily_max_calls:150}
const defaults:BusinessSchemeInput['limits']={total:{...limit},device:{...limit},target:{...limit}}
const defaultWindows=[{weekdays:[1,2,3,4,5,6,7],start_minute:0,end_minute:1440}]
export function BusinessSchemeEditor({scheme,unit}:{scheme?:BusinessScheme|null;unit?:string}){
 const cache=useQueryClient();const keys=useRef(new Map<string,string>())
 const [open,setOpen]=useState(false)
 const [form,writeForm]=useState<BusinessSchemeInput>({expected_revision:0,enabled:true,confirmed:false,timezone:'Asia/Shanghai',allowed_windows:structuredClone(defaultWindows),limits:defaults,reason:''})
 function setForm(value:BusinessSchemeInput){writeForm({...value,confirmed:false})}
 const save=useMutation({mutationFn:()=>{const body=JSON.stringify(form);let key=keys.current.get(body);if(!key){key=createIdempotencyKey();keys.current.set(body,key)}return sharedSend.saveBusinessScheme(form,key,unit)},onSuccess:()=>{setOpen(false);void cache.invalidateQueries({queryKey:['mobile-execution']})}})
 function begin(){setForm({expected_revision:scheme?.scope_revision??0,enabled:scheme?scheme.mode==='enabled':true,confirmed:false,timezone:scheme?.timezone??'Asia/Shanghai',allowed_windows:structuredClone(scheme?.allowed_windows??defaultWindows),limits:structuredClone(scheme?.limits??defaults),reason:''});save.reset();setOpen(true)}
 const invalid=form.allowed_windows.length===0||form.allowed_windows.some(w=>!w.weekdays.length||w.start_minute===w.end_minute)
 return <><Button onClick={begin}>配置业务方案</Button><Drawer title={unit?"单位手机群消息业务方案":"默认手机群消息业务方案"} open={open} size={920} onClose={()=>setOpen(false)} footer={<Button type="primary" loading={save.isPending} disabled={!form.confirmed||!form.reason.trim()||invalid} onClick={()=>save.mutate()}>确认保存</Button>}>
 <Space orientation="vertical" style={{width:'100%'}}>
 <Alert type="info" title="变更影响" description="单位专属方案完整覆盖默认设置；默认方案分别作用于各继承单位。关闭后任务仍可确认排队，停止后续签发。变更不会重置已用额度，已交付的执行包保留原约束。"/>
 <label>允许实际执行 <Switch aria-label="允许实际执行" checked={form.enabled} onChange={enabled=>setForm({...form,enabled})}/></label>
 <p>此开关只控制实际执行，不阻止任务提交和排队；手机运行条件由执行端处理。</p>
 <label>IANA 时区 <Input aria-label="业务方案时区" value={form.timezone} onChange={e=>setForm({...form,timezone:e.target.value})}/></label>
 <p>允许时段按开始日计算，跨午夜延续至次日；结束 00:00 表示当天 24:00。</p>
 {form.allowed_windows.map((w,i)=><Space key={i} wrap>
 <Select aria-label={`时段${i+1}星期`} mode="multiple" style={{minWidth:250}} value={w.weekdays} options={policyWeekdays} onChange={weekdays=>setForm({...form,allowed_windows:form.allowed_windows.map((v,n)=>n===i?{...v,weekdays}:v)})}/>
 {(['start_minute','end_minute'] as const).map(k=><Input key={k} aria-label={`时段${i+1}${k==='start_minute'?'开始':'结束'}`} type="time" value={policyClock(w[k])} onChange={e=>{const minute=policyMinute(e.target.value,k==='end_minute');if(minute!==undefined)setForm({...form,allowed_windows:form.allowed_windows.map((v,n)=>n===i?{...v,[k]:minute}:v)})}}/>)}
 <Button onClick={()=>setForm({...form,allowed_windows:form.allowed_windows.filter((_,n)=>n!==i)})}>删除</Button>
 </Space>)}
 <Button disabled={form.allowed_windows.length>=32} onClick={()=>setForm({...form,allowed_windows:[...form.allowed_windows,newPolicyWindow()]})}>添加时段</Button>
 <div style={{display:'grid',gridTemplateColumns:'repeat(auto-fit,minmax(280px,1fr))',gap:16,width:'100%'}}>
 {(Object.entries({total:'本业务单位总量',device:'本单位每台手机',target:'本单位每个目标群'}) as Array<[keyof BusinessSchemeInput['limits'],string]>).map(([scope,title])=><fieldset key={scope}><legend>{title}</legend><div style={{display:'grid',gridTemplateColumns:'1fr 1fr',gap:12}}>
 {(Object.entries({interval_seconds:'最小间隔（秒）',window_seconds:'滚动窗口（秒）',max_calls:'窗口最多调用数',daily_max_calls:'每日最多调用数'}) as Array<[keyof BusinessSchemeInput['limits']['total'],string]>).map(([field,label])=><label key={field}>{label}<InputNumber aria-label={`${title}${label}`} min={1} max={field==='window_seconds'?2592000:field==='interval_seconds'?86400:100000} precision={0} value={form.limits[scope][field]} onChange={v=>{if(v!==null)setForm({...form,limits:{...form.limits,[scope]:{...form.limits[scope],[field]:v}}})}}/></label>)}
 </div></fieldset>)}
 </div>
 <label>变更理由 <Input.TextArea aria-label="业务方案变更理由" maxLength={2000} value={form.reason} onChange={e=>setForm({...form,reason:e.target.value})}/></label>
 <Checkbox checked={form.confirmed} onChange={e=>writeForm({...form,confirmed:e.target.checked})}>已核对上述开关、时段、额度和变更影响，确认授权本次方案变更</Checkbox>
 {save.error&&<Alert type="error" title={save.error.message}/>}
 </Space></Drawer></>
}

export function BusinessSchemePage({unit,unitName}:{unit?:string;unitName?:string}){
 const cache=useQueryClient();const [reason,setReason]=useState('');const [confirmed,setConfirmed]=useState(false)
 const query=useQuery({queryKey:['mobile-execution','business-scheme',unit??'default'],queryFn:()=>sharedSend.businessScheme(unit)})
 const inherit=useMutation({mutationFn:()=>sharedSend.inheritBusinessScheme(unit!,{expected_revision:query.data!.scope_revision,reason,confirmed},createIdempotencyKey()),onSuccess:()=>{setReason('');setConfirmed(false);void cache.invalidateQueries({queryKey:['mobile-execution']})}})
 return <Card title={unit?`单位手机群消息业务方案：${unitName??unit}`:'默认手机群消息业务方案'}>
  {(query.error||inherit.error)&&<Alert type="error" title={query.error?.message||inherit.error?.message}/>}
  {query.data&&<Space orientation="vertical"><p>当前来源：{{builtin:'系统内置默认方案',default:'默认方案',unit:'单位专属方案'}[query.data.source]}；配置修订 {query.data.scope_revision}；{query.data.mode==='enabled'?'允许执行':'已暂停执行'}。</p><p>时区 {query.data.timezone}；每日最多 {query.data.limits.total.daily_max_calls} 次。各单位分别累计额度。</p>
  <BusinessSchemeEditor scheme={query.data} unit={unit}/>
  {unit&&query.data.source==='unit'&&<><Input aria-label="恢复默认理由" placeholder="恢复默认理由" value={reason} onChange={e=>{setReason(e.target.value);setConfirmed(false)}}/><Checkbox checked={confirmed} onChange={e=>setConfirmed(e.target.checked)}>确认恢复使用当前默认方案，不重置额度</Checkbox><Button disabled={!reason.trim()||!confirmed} loading={inherit.isPending} onClick={()=>inherit.mutate()}>恢复使用默认方案</Button></>}
  </Space>}
 </Card>
}
