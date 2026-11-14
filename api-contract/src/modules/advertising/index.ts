import {requestResource} from '../../http'
import {createAdPlacementClient,createScopedMaterialClient} from './material-client'
export * from './client'
export * from './material-client'
export const scopedMaterials=createScopedMaterialClient({requestResource})
export const adPlacements=createAdPlacementClient({requestResource})
