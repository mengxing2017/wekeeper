import { createCommunityClient } from '@wekeeper/api-contract/community'
import { client } from '../../identity/api'
const community = createCommunityClient(client)
export const feedbackApi = { submit: community.feedback }
