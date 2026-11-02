import type {SchoolResource} from './api'
type Action=SchoolResource['action']
export async function executeResourceAction(action:Action){
 switch(action.kind){
 case 'none':return
 case 'internal_page':{
  const paths={feed:'/pages/index/index',resources:'/pages/index/category',legal:'/pages/user/legal',post:`/pages/index/detail?id=${encodeURIComponent(action.object_id??'')}`}
  if(action.page==='feed')await uni.switchTab({url:paths.feed})
  else await uni.navigateTo({url:paths[action.page]})
  return
 }
 case 'external_link':await uni.setClipboardData({data:action.url});return
 case 'phone':await uni.makePhoneCall({phoneNumber:action.number});return
 case 'copy_text':await uni.setClipboardData({data:action.text});return
 }
}
export function actionLabel(action:Action){switch(action?.kind){case 'internal_page':return '打开页面';case 'external_link':return '复制链接';case 'phone':return '拨打电话';case 'copy_text':return '复制联系方式或说明';default:return ''}}
