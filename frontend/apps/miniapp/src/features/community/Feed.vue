<script setup lang="ts">
import { shallowRef, ref, computed, watch } from 'vue'
import { communityApi, type Post, type FeedFilter } from './api'
import { requireSchool, schoolContext } from '../school-context/context'
import { session } from '../identity/session'
import { usePage, navigate } from '../../shared/use-page'
import PageStatus from '../../shared/PageStatus.vue'
import PrivateImage from '../media/PrivateImage.vue'
const props = defineProps<{
  initial?: 'feed' | 'following' | 'mine' | 'collect'
  searchPage?: boolean
}>()
const mode = ref(props.initial ?? 'feed'),
  search = ref(''),
  posts = shallowRef<Post[]>([]),
  page = ref(1),
  more = ref(false)
watch(
  () => props.initial,
  (value) => {
    mode.value = value ?? 'feed'
  },
)
const school = computed(() => schoolContext.schools.find((s) => s.id === schoolContext.id))
const { busy, error, run } = usePage(() => {
  posts.value = []
  page.value = 1
  more.value = false
})
function load(reset = true) {
  return run(async (current) => {
    if (!(await requireSchool())) return
    const id = schoolContext.id,
      generation = schoolContext.generation,
      user = session.user?.id
    const next = reset ? 1 : page.value + 1
    const kind: FeedFilter['kind'] =
      mode.value === 'collect' || mode.value === 'following' ? mode.value : undefined
    const response = await communityApi.posts(id, {
      page: next,
      search: search.value,
      mine: mode.value === 'mine',
      kind,
    })
    if (
      current() &&
      generation === schoolContext.generation &&
      id === schoolContext.id &&
      user === session.user?.id
    ) {
      posts.value = reset ? response.items : [...posts.value, ...response.items]
      page.value = next
      more.value = !!response.page?.has_more
    }
  })
}
function setMode(value: typeof mode.value) {
  if (busy.value) return
  mode.value = value
  void load()
}
function detail(post: Post) {
  navigate(`/pages/index/detail?school=${schoolContext.id}&id=${post.id}`)
}
defineExpose({
  show: () =>
    posts.value.length
      ? run(async () => {
          await requireSchool()
        })
      : load(),
  refresh: load,
  loadMore: () => {
    if (more.value) return load(false)
  },
})
</script>
<template>
  <view class="page">
    <view class="row spread">
      <text class="heading">{{ school?.name || '校园社区' }}</text>
      <button size="mini" @tap="navigate('/pages/plugin/index')">选校</button>
    </view>
    <view class="row tabs">
      <button
        size="mini"
        :disabled="busy"
        :class="{ selected: mode === 'feed' }"
        @tap="setMode('feed')"
      >
        发现
      </button>
      <button
        size="mini"
        :disabled="busy"
        :class="{ selected: mode === 'following' }"
        @tap="setMode('following')"
      >
        关注
      </button>
      <button
        size="mini"
        :disabled="busy"
        :class="{ selected: mode === 'collect' }"
        @tap="setMode('collect')"
      >
        收藏
      </button>
      <button
        size="mini"
        :disabled="busy"
        :class="{ selected: mode === 'mine' }"
        @tap="setMode('mine')"
      >
        我的发布
      </button>
    </view>
    <view class="row">
      <input
        v-model="search"
        :disabled="busy"
        class="input grow"
        placeholder="搜索校园内容"
        confirm-type="search"
        @confirm="load()"
      />
      <button size="mini" @tap="load()">搜索</button>
    </view>
    <view class="waterfall">
      <view v-for="post in posts" :key="post.id" class="card post-card" @tap="detail(post)">
        <PrivateImage v-if="post.images[0]" :school="schoolContext.id" :source="post.images[0]" />
        <text class="title">{{ post.title }}</text>
        <text class="summary">{{ post.content }}</text>
        <view class="row spread muted">
          <text>{{ post.author_name || '校园成员' }}</text>
          <text>♡ {{ post.like_count || 0 }}</text>
        </view>
        <text v-if="mode === 'mine'" class="muted">
          {{
            post.audit_status === 'pending'
              ? '待审核'
              : post.audit_status === 'rejected'
                ? '未通过审核'
                : '已通过审核'
          }}
        </text>
      </view>
    </view>
    <PageStatus :busy="busy" :error="error" :empty="!posts.length" @retry="load()" />
    <button v-if="more" :disabled="busy" @tap="load(false)">加载更多</button>
  </view>
</template>
