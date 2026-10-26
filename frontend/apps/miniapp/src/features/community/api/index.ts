import { createCommunityClient } from '@wekeeper/api-contract/community'
import { client } from '../../identity/api'
export type {
  Post,
  PostInput,
  Comment,
  Reaction,
  Follow,
  Notification,
  FeedFilter,
  SchoolResource,
} from '@wekeeper/api-contract/community'
export const communityApi = createCommunityClient(client)
