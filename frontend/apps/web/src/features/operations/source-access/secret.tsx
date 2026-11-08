import {useRef,useState} from 'react'
import {Alert,Button,Input,Space,Typography} from 'antd'
import {sourceAccess,type CredentialSecret} from './api'

export function CallerSecret({keyId}:{keyId:string}) {
 const [material,setMaterial]=useState<CredentialSecret>(),[busy,setBusy]=useState(false),[error,setError]=useState('')
 const request=useRef(0)
 const hide=()=>{request.current++;setMaterial(undefined);setError('')}
 const reveal=async()=>{
  const current=++request.current
  setBusy(true);setError('')
  try{const value=await sourceAccess.revealSecret(keyId);if(current===request.current)setMaterial(value)}
  catch(e){if(current===request.current)setError((e as Error).message)}
  finally{setBusy(false)}
 }
 const download=()=>{
  if(!material)return
  const url=URL.createObjectURL(new Blob([JSON.stringify({key_id:material.key_id,secret:material.secret},null,2)],{type:'application/json'}))
  const link=document.createElement('a');link.href=url;link.download=`wekeeper-credential-${keyId}.json`;link.click();URL.revokeObjectURL(url)
 }
 return <Space orientation="vertical">
  {error&&<Alert type="error" title={error}/>}
  {material?<>
   <Input.TextArea aria-label="签名密钥（secret）" readOnly value={material.secret} autoComplete="off" rows={2}/>
   <Space><Typography.Text copyable={{text:material.secret}}>复制密钥</Typography.Text><Button onClick={download}>下载凭据</Button><Button onClick={hide}>隐藏密钥</Button></Space>
  </>:<Button loading={busy} onClick={reveal}>查看密钥</Button>}
  <span>可重复查看和核对同一密钥。每次查看会记录操作审计。</span>
 </Space>
}
