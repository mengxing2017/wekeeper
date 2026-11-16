import {useQuery} from '@tanstack/react-query'
import {Alert,Space,Tabs} from 'antd'
import {sourceRouting} from './api'
import {ReceivingConfigurations} from '../receiving-configurations'
export function RouteConfigurationEditor({route}:{route:string}){
 const q=useQuery({queryKey:['route-configuration',route],queryFn:()=>sourceRouting.configuration(route)})
 return <Space orientation="vertical" style={{width:'100%'}}>
 {q.error&&<Alert type="error" title={q.error.message}/>}
 {q.data&&<><p>业务单位：{q.data.name}</p><Tabs items={(['mobile','wecom'] as const).map(channel=>({key:channel,label:channel==='mobile'?'手机分发':'企业微信',children:<ReceivingConfigurations channel={channel} scope={q.data!.business_unit_id} admin platformAdmin/>}))}/></>}
 </Space>
}
