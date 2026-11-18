import {Button,Table} from 'antd'
import type {Bucket} from './api'
const formatDate=(v:string)=>new Date(v).toLocaleString('zh-CN',{timeZone:'Asia/Shanghai'})
export const count=(v:string)=>BigInt(v).toLocaleString('zh-CN')
export function Trend({buckets,onSelect}:{buckets:Bucket[];onSelect:(start:string)=>void}) {
 const width=900,height=210,pad=25
 const values=buckets.map(b=>BigInt(b.received))
 const maximum=values.reduce((a,b)=>a>b?a:b,1n)
 const xy=(i:number)=>`${pad+i*(width-2*pad)/Math.max(1,buckets.length-1)},${height-pad-Number(values[i]*10000n/maximum)/10000*(height-2*pad)}`
 return <section aria-label="接收消息趋势图">
  <svg viewBox={`0 0 ${width} ${height}`} role="img" aria-label="接收趋势（条），完整数值见下方周期表" style={{width:'100%',maxHeight:260}}>
   <path d={`M${pad},${pad} V${height-pad} H${width-pad}`} stroke="#aaa" fill="none"/>
   <polyline points={buckets.map((_,i)=>xy(i)).join(' ')} fill="none" stroke="#1677ff" strokeWidth="2"/>
   {buckets.map((b,i)=><circle key={b.start} cx={xy(i).split(',')[0]} cy={xy(i).split(',')[1]} r="4" fill="#1677ff" role="button" tabIndex={0} aria-label={`${formatDate(b.start)} 接收 ${count(b.received)} 条`} onClick={()=>onSelect(b.start)} onKeyDown={e=>{if(e.key==='Enter'||e.key===' '){e.preventDefault();onSelect(b.start)}}}><title>{formatDate(b.start)}：{count(b.received)} 条</title></circle>)}
  </svg>
  <Table<Bucket> rowKey="start" size="small" dataSource={buckets} pagination={{pageSize:10}} scroll={{x:800}} columns={[
   {title:'周期起点（北京时间）',dataIndex:'start',render:(v:string)=><Button type="link" onClick={()=>onSelect(v)}>{formatDate(v)}</Button>},
   {title:'接收（条）',dataIndex:'received',render:count},
   ...(['total','sent','failed','unknown','pending'] as const).map((key,i)=>({title:['目标（项）','成功','失败','未知','处理中'][i],render:(_:unknown,b:Bucket)=>count(b.targets[key])})),
  ]}/>
 </section>
}

export function OutcomeTrend({buckets,onSelect}:{buckets:Bucket[];onSelect:(bucket:string,outcome:'sent'|'failed'|'unknown'|'pending')=>void}) {
 const keys=['sent','failed','unknown','pending'] as const
 const labels=['成功','失败','未知','处理中'],colors=['#389e0d','#cf1322','#d48806','#1677ff']
 const maximum=buckets.reduce((m,b)=>BigInt(b.targets.total)>m?BigInt(b.targets.total):m,1n)
 const step=850/Math.max(1,buckets.length)
 return <section aria-label="目标结果趋势图"><svg viewBox="0 0 900 220" role="img" aria-label="群目标结果堆叠柱图，完整数值见接收趋势周期表" style={{width:'100%',maxHeight:260}}>
  {buckets.flatMap((b,i)=>{let top=195;return keys.map((key,k)=>{const h=Number(BigInt(b.targets[key])*17000n/maximum)/100;top-=h;return <rect key={`${b.start}:${key}`} x={25+i*step} y={top} width={Math.max(1,step*0.75)} height={h} fill={colors[k]} role="button" tabIndex={0} aria-label={`${formatDate(b.start)} ${labels[k]} ${count(b.targets[key])} 项`} onClick={()=>onSelect(b.start,key)} onKeyDown={e=>{if(e.key==='Enter'||e.key===' '){e.preventDefault();onSelect(b.start,key)}}}><title>{formatDate(b.start)} {labels[k]} {count(b.targets[key])} 项</title></rect>})})}
 </svg><p>{keys.map((key,k)=><span key={key} style={{marginRight:16,color:colors[k]}}>{labels[k]}</span>)}</p></section>
}
