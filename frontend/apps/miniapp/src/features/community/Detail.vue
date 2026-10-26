<script setup lang="ts">
import { shallowRef, ref } from 'vue'
import { communityApi, type Post, type Comment, type Reaction, type Follow } from './api'
import { requireSchool, scopeKey } from '../school-context/context'
import { session } from '../identity/session'
import { usePage, confirm, navigate } from '../../shared/use-page'
import PrivateImage from '../media/PrivateImage.vue'
import PageStatus from '../../shared/PageStatus.vue'
const props = defineProps<{ id: string; school: string }>()
const post = shallowRef<Post>(),
  comments = shallowRef<Comment[]>([]),
  reactions = shallowRef<Reaction[]>([]),
  follows = shallowRef<Follow[]>([]),
  page = ref(1),
  more = ref(false),
  content = ref(''),
  parent = ref<string>(),
  reason = ref('')
const { busy, error, run } = usePage(() => {
  post.value = undefined
  comments.value = []
  reactions.value = []
  follows.value = []
  content.value = ''
  parent.value = undefined
})
async function read() {
  if (!props.id || !props.school) throw new Error('帖子链接无效')
  if (!(await requireSchool(props.school))) return
  const key = scopeKey()
  const [p, c, r, f] = await Promise.all([
    communityApi.post(props.school, props.id),
    communityApi.comments(props.school, props.id),
    communityApi.reactions(props.school),
    communityApi.follows(props.school),
  ])
  if (key !== scopeKey()) return
  post.value = p
  comments.value = c.items
  page.value = 1
  more.value = !!c.page?.has_more
  reactions.value = r
  follows.value = f
}
function load() {
  return run(() => read())
}
const active = (kind: 'like' | 'collect') =>
  reactions.value.some((r) => r.post_id === props.id && r.kind === kind)
function react(kind: 'like' | 'collect') {
  void run(async (current) => {
    await communityApi.react(props.school, props.id, kind, !active(kind))
    if (current()) await read()
  })
}
function follow() {
  void run(async (current) => {
    await communityApi.follow(
      props.school,
      post.value!.author_id,
      !follows.value.some((f) => f.id === post.value!.author_id),
    )
    if (current()) await read()
  })
}
function comment() {
  void run(async (current) => {
    if (!content.value.trim()) throw new Error('请填写评论内容')
    await communityApi.comment(props.school, props.id, content.value, parent.value)
    content.value = ''
    parent.value = undefined
    if (current()) await read()
  })
}
function remove() {
  void run(async () => {
    if (post.value && (await confirm('确认删除这条动态？'))) {
      await communityApi.remove(props.school, post.value)
      uni.navigateBack()
    }
  })
}
function removeComment(item: Comment) {
  void run(async (current) => {
    if (await confirm('确认删除评论？')) {
      await communityApi.deleteComment(props.school, item)
      if (current()) await read()
    }
  })
}
function report() {
  void run(async () => {
    if (!reason.value.trim()) throw new Error('请填写举报原因')
    await communityApi.report(props.school, props.id, reason.value)
    reason.value = ''
    uni.showToast({ title: '已提交举报', icon: 'none' })
  })
}
function next() {
  void run(async (current) => {
    const response = await communityApi.comments(props.school, props.id, page.value + 1)
    if (current()) {
      comments.value = [...comments.value, ...response.items]
      page.value++
      more.value = !!response.page?.has_more
    }
  })
}
defineExpose({
  show: load,
  share: () => ({
    title: post.value?.title ?? '校园动态',
    path: `/pages/index/detail?school=${props.school}&id=${props.id}`,
  }),
})
</script>
<template>
  <view class="page">
    <PageStatus :busy="busy" :error="error" @retry="load" />
    <template v-if="post">
      <view class="card">
        <text class="heading">{{ post.title }}</text>
        <text class="muted">{{ post.author_name || '校园成员' }}</text>
        <text class="body">{{ post.content }}</text>
        <PrivateImage v-for="src in post.images" :key="src" :school="props.school" :source="src" />
        <view class="row">
          <text v-for="tag in post.tags" :key="tag" class="tag">#{{ tag }}</text>
        </view>
        <text class="muted">
          {{
            post.audit_status === 'approved'
              ? '已通过审核'
              : post.audit_status === 'pending'
                ? '待审核'
                : '未通过审核'
          }}
        </text>
        <view class="row">
          <button
            size="mini"
            :disabled="busy || post.audit_status !== 'approved' || post.visibility !== 'visible'"
            @tap="react('like')"
          >
            {{ active('like') ? '取消点赞' : '点赞' }}
          </button>
          <button
            size="mini"
            :disabled="busy || post.audit_status !== 'approved' || post.visibility !== 'visible'"
            @tap="react('collect')"
          >
            {{ active('collect') ? '取消收藏' : '收藏' }}
          </button>
          <button
            v-if="post.author_id !== session.user?.id"
            size="mini"
            :disabled="busy"
            @tap="follow"
          >
            {{ follows.some((f) => f.id === post!.author_id) ? '取消关注' : '关注作者' }}
          </button>
          <button
            v-if="post.author_id === session.user?.id"
            size="mini"
            @tap="navigate(`/pages/community/edit?school=${props.school}&id=${props.id}`)"
          >
            编辑
          </button>
          <button
            v-if="post.author_id === session.user?.id"
            size="mini"
            :disabled="busy"
            @tap="remove"
          >
            删除
          </button>
        </view>
      </view>
      <view class="card">
        <text class="title">评论</text>
        <view v-for="item in comments" :key="item.id" class="comment">
          <text>{{ item.author_name }}：{{ item.content }}</text>
          <view class="row">
            <button size="mini" @tap="parent = item.id">回复</button>
            <button
              v-if="item.author_id === session.user?.id"
              size="mini"
              :disabled="busy"
              @tap="removeComment(item)"
            >
              删除
            </button>
          </view>
        </view>
        <button v-if="more" :disabled="busy" @tap="next">更多评论</button>
        <text v-if="parent" class="muted" @tap="parent = undefined">正在回复评论，点击取消</text>
        <textarea
          v-model="content"
          class="input"
          maxlength="2000"
          placeholder="友善交流，留下你的评论"
        />
        <button class="primary" :disabled="busy" @tap="comment">提交评论</button>
      </view>
      <view class="card">
        <input v-model="reason" class="input" placeholder="举报原因" maxlength="500" />
        <button size="mini" :disabled="busy" @tap="report">举报动态</button>
      </view>
    </template>
  </view>
</template>
