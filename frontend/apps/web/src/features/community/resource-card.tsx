import {useState} from 'react'
import {Alert,Button,Card,Image} from 'antd'
import {Link} from 'react-router-dom'
import type {PublishedResource,ResourceAction} from './api'
export function internalResourcePath(school:string,action:Extract<ResourceAction,{kind:'internal_page'}>){switch(action.page){case 'post':return `/community/${school}/posts/${action.object_id}`;case 'feed':return `/community/${school}`;case 'resources':return `/community/${school}?service=resources`;case 'legal':return `/community/${school}?service=legal`;}}
export function ResourceCard({school,item,onAction}:{school:string;item:PublishedResource;onAction?:()=>Promise<unknown>}){
 const [error,setError]=useState(''),[busy,setBusy]=useState(false)
 async function copy(value:string){setBusy(true);setError('');try{await navigator.clipboard.writeText(value);await onAction?.()}catch(e){setError(e instanceof Error?e.message:'操作失败，请重试')}finally{setBusy(false)}}
 const action=item.action??{kind:'none'}
 return <Card title={item.title}><p className="text-content">{item.content}</p>{item.image_url&&<Image src={item.image_url} alt={item.kind==='qrcode'?'二维码':item.title} style={{maxWidth:'100%'}}/>}
 {action.kind==='internal_page'&&<Link to={internalResourcePath(school,action)} onClick={()=>void onAction?.()}>打开{item.title}</Link>}
 {action.kind==='external_link'&&<Button disabled={busy} onClick={()=>void copy(action.url)}>复制链接</Button>}
 {action.kind==='phone'&&<a href={`tel:${action.number}`} onClick={()=>void onAction?.()}>拨打 {action.number}</a>}
 {action.kind==='copy_text'&&<Button disabled={busy} onClick={()=>void copy(action.text)}>复制联系方式或说明</Button>}
 {error&&<Alert type="error" title={error}/>}</Card>
}
