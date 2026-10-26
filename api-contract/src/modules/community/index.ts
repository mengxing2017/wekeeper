import type { Client, Cancellation } from '../../http/client'
import { schoolPath } from '../schools'
import type { Post, Row } from '../business/types'
export type { Post, Row } from '../business/types'
export interface PostInput {
  title: string
  content: string
  images: string[]
  tags: string[]
  revision?: number
}
export interface Comment extends Row {
  author_id: string
  author_name: string
  content: string
  revision: number
  parent_id: string | null
}
export interface Reaction extends Row {
  post_id: string
  kind: 'like' | 'collect'
}
export interface Follow {
  id: string
  display_name: string
}
export interface Notification extends Row {
  kind: string
  object_id: string
  created_at: string
  read_at: string | null
}
export interface FeedFilter {
  page?: number
  search?: string
  mine?: boolean
  kind?: 'collect' | 'following'
}
export interface SchoolResource extends Row {
  kind: string
  title: string
  content: string
  url: string
}
export function pageQuery(page = 1) {
  return `page[number]=${page}&page[size]=20`
}
export function createCommunityClient(client: Client) {
  const scope = (s: string, p: string) => `${schoolPath(s)}/${p}`
  const part = encodeURIComponent
  return {
    posts(s: string, filter: FeedFilter = {}, signal?: Cancellation) {
      const query = `${pageQuery(filter.page)}&search=${part(filter.search ?? '')}&mine=${!!filter.mine}${filter.kind ? `&kind=${filter.kind}` : ''}`
      return client.requestCollection<Post>(scope(s, 'posts') + '?' + query, signal)
    },
    post: (s: string, id: string, signal?: Cancellation) =>
      client.requestResource<Post>(scope(s, `posts/${part(id)}`), 'GET', undefined, signal),
    save: (s: string, input: PostInput, id?: string) =>
      client.requestResource<Post>(
        scope(s, id ? `posts/${part(id)}` : 'posts'),
        id ? 'PUT' : 'POST',
        input,
      ),
    remove: (s: string, post: Pick<Post, 'id' | 'revision'>) =>
      client.requestResource<Post>(scope(s, `posts/${part(post.id)}/visibility`), 'PUT', {
        revision: post.revision,
        visibility: 'deleted',
        top_sort: 0,
      }),
    comments: (s: string, id: string, page = 1) =>
      client.requestCollection<Comment>(
        scope(s, `posts/${part(id)}/comments`) + `?${pageQuery(page)}`,
      ),
    comment: (s: string, id: string, content: string, parent_id?: string) =>
      client.requestResource<Comment>(scope(s, `posts/${part(id)}/comments`), 'POST', {
        content,
        parent_id,
      }),
    deleteComment: (s: string, comment: Pick<Comment, 'id' | 'revision'>) =>
      client.requestResource<null>(scope(s, `comments/${part(comment.id)}`), 'DELETE', {
        revision: comment.revision,
      }),
    reactions: (s: string) => client.requestResource<Reaction[]>(scope(s, 'reactions')),
    react: (s: string, id: string, kind: 'like' | 'collect', active: boolean) =>
      client.requestResource<null>(
        scope(s, `posts/${part(id)}/reactions/${kind}`),
        active ? 'PUT' : 'DELETE',
      ),
    follows: (s: string) => client.requestResource<Follow[]>(scope(s, 'follows')),
    follow: (s: string, id: string, active: boolean) =>
      client.requestResource<null>(scope(s, `follows/${part(id)}`), active ? 'PUT' : 'DELETE'),
    notifications: (s: string, page = 1) =>
      client.requestCollection<Notification>(scope(s, 'notifications') + `?${pageQuery(page)}`),
    read: (s: string, id: string) =>
      client.requestResource<null>(scope(s, `notifications/${part(id)}/read`), 'PUT'),
    resources: (s: string) => client.requestResource<SchoolResource[]>(scope(s, 'resources')),
    click: (s: string, id: string) =>
      client.requestResource<null>(scope(s, `resources/${part(id)}/click`), 'POST'),
    report: (s: string, post_id: string, content: string) =>
      client.requestResource<Row>(scope(s, 'cases'), 'POST', {
        kind: 'report',
        post_id,
        content,
      }),
    feedback: (s: string, content: string, contact: string, images: string[] = []) =>
      client.requestResource<Row>(scope(s, 'cases'), 'POST', {
        kind: 'feedback',
        content,
        contact,
        images,
      }),
  }
}
