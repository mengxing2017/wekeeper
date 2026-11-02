import { UsageManagement } from '../../admin/mobile-resources/usage-management'
export function MobileTargetBindings({school,admin=false}:{school:string;admin?:boolean}) {return <UsageManagement school={school} admin={admin}/>}
