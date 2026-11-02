// Browser facade; native/miniapp consumers inject a separate transport.
import {requestResource,requestCollection} from '../../http';
import {createWecomDistributionClient} from './client';
export * from './client';
export const {broadcast,officialSenders,distributionPolicies}=createWecomDistributionClient({requestResource,requestCollection});

export * from '../product-workflows'
