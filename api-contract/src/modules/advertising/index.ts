import {requestResource} from '../../http'
import {createAdvertisingClient} from './client'
import {createPlacementRuleClient,createScopedMaterialClient} from './material-client'
export * from './client'
export * from './material-client'
export const advertising=createAdvertisingClient({requestResource})
export const scopedMaterials=createScopedMaterialClient({requestResource})
export const placementRules=createPlacementRuleClient({requestResource})
