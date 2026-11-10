import {describe,expect,it} from 'vitest'
import {parseKeywordCsv} from './keyword-import'

describe('敏感词 CSV 导入',()=>{
 it('解析 BOM、引号、换行和启停状态',()=>{
  expect(parseKeywordCsv('\ufeffword,enabled\r\n"敏感,词",false\r\n"跨\n行",1',[])).toEqual([
   {line:2,word:'敏感,词',enabled:false,error:''},
   {line:3,word:'跨\n行',enabled:true,error:''},
  ])
 })
 it('报告已有词、文件内重复及无效行',()=>{
  const rows=parseKeywordCsv('word,enabled\nＡ,true\na,true\n,yes\nx,no',[{word:'a',enabled:true}])
  expect(rows.map(x=>x.error)).toEqual(['与已有词条或文件内词条重复','与已有词条或文件内词条重复','敏感词不能为空','enabled 须为 true、false、1 或 0'])
 })
 it('拒绝格式错误和错误表头',()=>{
  expect(()=>parseKeywordCsv('word\n"未结束',[])).toThrow('引号未闭合')
  expect(()=>parseKeywordCsv('other\na',[])).toThrow('表头')
 })
})
