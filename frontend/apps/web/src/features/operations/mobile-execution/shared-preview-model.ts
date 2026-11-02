import type { SharedPreview } from './api'

export function previewSelections(items: SharedPreview['plan']['items'], target: string, device: string) {
  return [...items.filter(i=>i.selected && i.target_id!==target).map(i=>({target_id:i.target_id,device_id:i.selected!.device_id})),{target_id:target,device_id:device}]
}
