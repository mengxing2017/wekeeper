import {useRef,useState} from 'react'
import {useMutation,useQueryClient} from '@tanstack/react-query'
import {Alert,Button,Checkbox,Drawer,Input,InputNumber,Select,Space,Switch} from 'antd'
import {sharedSend,type BusinessScheme,type BusinessSchemeInput,type SendingStatus} from './api'
import {createIdempotencyKey} from '../../../shared/idempotency-key'
import {newPolicyWindow,policyClock,policyMinute,policyWeekdays} from './shared-policy-time'
const defaults:BusinessSchemeInput['limits']={global:{interval_seconds:5,window_seconds:3600,max_calls:120,daily_max_calls:1000},school:{interval_seconds:10,window_seconds:3600,max_calls:60,daily_max_calls:300},device:{interval_seconds:30,window_seconds:3600,max_calls:30,daily_max_calls:150},target:{interval_seconds:60,window_seconds:3600,max_calls:10,daily_max_calls:30}}
export function BusinessSchemeEditor({scheme}:{scheme?:BusinessScheme|null;status?:SendingStatus}){
 const cache=useQueryClient();const keys=useRef(new Map<string,string>())
 const [open,setOpen]=useState(false)
 const [form,writeForm]=useState<BusinessSchemeInput>({expected_revision:0,enabled:false,confirmed:false,timezone:'Asia/Shanghai',allowed_windows:[newPolicyWindow()],limits:defaults,reason:''})
 function setForm(value:BusinessSchemeInput){writeForm({...value,confirmed:false})}
 const save=useMutation({mutationFn:()=>{const body=JSON.stringify(form);let key=keys.current.get(body);if(!key){key=createIdempotencyKey();keys.current.set(body,key)}return sharedSend.saveBusinessScheme(form,key)},onSuccess:()=>{setOpen(false);void cache.invalidateQueries({queryKey:['mobile-execution']})}})
 function begin(){setForm({expected_revision:scheme?.revision??0,enabled:scheme?.mode==='enabled',confirmed:false,timezone:scheme?.timezone??'Asia/Shanghai',allowed_windows:structuredClone(scheme?.allowed_windows??[newPolicyWindow()]),limits:structuredClone(scheme?.limits??defaults),reason:''});save.reset();setOpen(true)}
 const invalid=form.allowed_windows.length===0||form.allowed_windows.some(w=>!w.weekdays.length||w.start_minute===w.end_minute)
 return <><Button onClick={begin}>配置业务方案</Button><Drawer title="手机群消息业务方案" open={open} size={920} onClose={()=>setOpen(false)} footer={<Button type="primary" loading={save.isPending} disabled={!form.confirmed||!form.reason.trim()||invalid} onClick={()=>save.mutate()}>确认保存业务方案</Button>}>
 <Space orientation="vertical" style={{width:'100%'}}>
 <Alert type="info" title="变更影响" description="时段、额度和间隔变化需要重新核对使用授权；关闭后停止新许可。已有调用及不确定结果保留，不会重置累计额度。技术产品升级不会覆盖此方案。"/>
 <label>开放新发送 <Switch aria-label="开放新发送" checked={form.enabled} onChange={enabled=>setForm({...form,enabled})}/></label>
 <p>发送开关只管理平台是否允许发送；实际执行时由系统自动判断手机是否就绪。</p>
 <label>IANA 时区 <Input aria-label="业务方案时区" value={form.timezone} onChange={e=>setForm({...form,timezone:e.target.value})}/></label>
 <p>允许时段按开始日计算，跨午夜延续至次日；结束 00:00 表示当天 24:00。</p>
 {form.allowed_windows.map((w,i)=><Space key={i} wrap>
 <Select aria-label={`时段${i+1}星期`} mode="multiple" style={{minWidth:250}} value={w.weekdays} options={policyWeekdays} onChange={weekdays=>setForm({...form,allowed_windows:form.allowed_windows.map((v,n)=>n===i?{...v,weekdays}:v)})}/>
 {(['start_minute','end_minute'] as const).map(k=><Input key={k} aria-label={`时段${i+1}${k==='start_minute'?'开始':'结束'}`} type="time" value={policyClock(w[k])} onChange={e=>{const minute=policyMinute(e.target.value,k==='end_minute');if(minute!==undefined)setForm({...form,allowed_windows:form.allowed_windows.map((v,n)=>n===i?{...v,[k]:minute}:v)})}}/>)}
 <Button onClick={()=>setForm({...form,allowed_windows:form.allowed_windows.filter((_,n)=>n!==i)})}>删除时段</Button>
 </Space>)}
 <Button disabled={form.allowed_windows.length>=32} onClick={()=>setForm({...form,allowed_windows:[...form.allowed_windows,newPolicyWindow()]})}>添加允许时段</Button>
 <div style={{display:'grid',gridTemplateColumns:'repeat(auto-fit,minmax(280px,1fr))',gap:16,width:'100%'}}>
 {(Object.entries({global:'平台全局',school:'每所学校',device:'每台手机',target:'每个目标群'}) as Array<[keyof BusinessSchemeInput['limits'],string]>).map(([scope,title])=><fieldset key={scope}><legend>{title}</legend><div style={{display:'grid',gridTemplateColumns:'1fr 1fr',gap:12}}>
 {(Object.entries({interval_seconds:'最小间隔（秒）',window_seconds:'滚动窗口（秒）',max_calls:'窗口最多调用数',daily_max_calls:'每日最多调用数'}) as Array<[keyof BusinessSchemeInput['limits']['global'],string]>).map(([field,label])=><label key={field}>{label}<InputNumber aria-label={`${title}${label}`} min={1} max={field==='window_seconds'?2592000:field==='interval_seconds'?86400:100000} precision={0} value={form.limits[scope][field]} onChange={v=>{if(v!==null)setForm({...form,limits:{...form.limits,[scope]:{...form.limits[scope],[field]:v}}})}}/></label>)}
 </div></fieldset>)}
 </div>
 <label>变更理由 <Input.TextArea aria-label="业务方案变更理由" maxLength={2000} value={form.reason} onChange={e=>setForm({...form,reason:e.target.value})}/></label>
 <Checkbox checked={form.confirmed} onChange={e=>writeForm({...form,confirmed:e.target.checked})}>已核对上述开关、时段、额度和变更影响，确认授权本次方案变更</Checkbox>
 {save.error&&<Alert type="error" title={save.error.message}/>}
 </Space></Drawer></>
}
