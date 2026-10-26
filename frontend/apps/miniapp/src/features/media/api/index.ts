import { createMediaClient } from '@wekeeper/api-contract/media'
import { client } from '../../identity/api'
export const mediaApi = createMediaClient(client)
