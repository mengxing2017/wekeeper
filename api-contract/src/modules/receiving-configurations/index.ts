import {requestResource, requestCollection} from '../../http'
import {createReceivingConfigurationClient} from './client'
export * from './client'
export const mobileReceivingConfigurations = createReceivingConfigurationClient({requestResource, requestCollection}, 'mobile')
export const wecomReceivingConfigurations = createReceivingConfigurationClient({requestResource, requestCollection}, 'wecom')
