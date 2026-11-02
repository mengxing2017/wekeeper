import {GenericCapability} from './generic-capability'
import { SharedSendReadiness } from './shared-send-readiness'

/** Shared sending has one delegation and one automatic inspection path. */
export function DeviceCapabilities() {
  return <><SharedSendReadiness /><GenericCapability /></>
}
