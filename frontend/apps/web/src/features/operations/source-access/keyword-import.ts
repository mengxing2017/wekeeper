import type {KeywordEntry} from './api'

export type ImportRow = {line:number; word:string; enabled:boolean; error:string}

// Parse RFC 4180 fields, including quoted commas and embedded newlines.
export function parseKeywordCsv(input:string, existing:KeywordEntry[]):ImportRow[] {
 const rows:{line:number; fields:string[]}[]=[]
 let fields:string[]=[], field='', quoted=false, closed=false, line=1, start=1
 const text=input.replace(/^\uFEFF/,'')
 for(let i=0;i<text.length;i++){
  const c=text[i]
  if(quoted){
   if(c==='"'){if(text[i+1]==='"'){field+='"';i++}else{quoted=false;closed=true}}
   else {field+=c;if(c==='\n')line++}
  }else if(c==='"'&&field===''&&!closed){quoted=true}
  else if(c===','){fields.push(field);field='';closed=false}
  else if(c==='\n'||c==='\r'){
   if(c==='\r'&&text[i+1]==='\n')i++
   fields.push(field);rows.push({line:start,fields});fields=[];field='';closed=false;line++;start=line
  }else {if(closed||c==='"')throw new Error(`第 ${line} 行 CSV 格式错误`);field+=c}
 }
 if(quoted)throw new Error(`第 ${start} 行引号未闭合`)
 if(field!==''||fields.length){fields.push(field);rows.push({line:start,fields})}
 if(!rows.length)throw new Error('文件为空')
 const header=rows.shift()!.fields.map(x=>x.trim().toLowerCase())
 const wordIndex=header.indexOf('word'),enabledIndex=header.indexOf('enabled')
 if(wordIndex<0||new Set(header).size!==header.length||header.some(x=>!['word','enabled'].includes(x)))throw new Error('表头必须包含 word，可选 enabled')
 if(rows.length>2000)throw new Error('导入文件最多 2000 行')
 const key=(s:string)=>s.normalize('NFKC').toLowerCase()
 const seen=new Set(existing.map(x=>key(x.word)))
 return rows.map(({line:rowLine,fields:values},index)=>{
  const word=values[wordIndex]??'', state=enabledIndex<0?'true':(values[enabledIndex]??'').trim().toLowerCase()
  let error=''
  if(values.length!==header.length)error='列数不匹配'
  else if(!word.trim())error='敏感词不能为空'
  else if(new TextEncoder().encode(word).length>512||Array.from(word.normalize('NFKC')).length>128)error='敏感词过长'
  else if(!['true','false','1','0'].includes(state))error='enabled 须为 true、false、1 或 0'
  else if(seen.has(key(word)))error='与已有词条或文件内词条重复'
  else if(existing.length+index+1>2000)error='超过范围上限 2000 条'
  seen.add(key(word))
  return {line:rowLine,word,enabled:state==='true'||state==='1',error}
 })
}
