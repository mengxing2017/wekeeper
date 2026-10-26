<script setup lang="ts">
import { onLaunch } from '@dcloudio/uni-app'
import { credentials } from './app/credentials'
onLaunch(() => {
  credentials.restore()
})
</script>
<style>
/* Carry forward the old uniapp theme palette and card/feed interaction. */
page {
  background: #f5f7fb;
  color: #111827;
  font-size: 28rpx;
  --primary: #5b8ff9;
  --secondary: #48c5bd;
}
.page {
  padding: 28rpx;
  padding-bottom: 60rpx;
}
.heading {
  display: block;
  font-size: 42rpx;
  font-weight: 700;
  margin: 20rpx 0;
}
.title {
  display: block;
  font-size: 32rpx;
  font-weight: 600;
  margin: 16rpx 0;
}
.brand {
  display: block;
  font-size: 60rpx;
  color: var(--primary);
  font-weight: 800;
  margin-top: 100rpx;
}
.card {
  background: #fff;
  border-radius: 22rpx;
  padding: 26rpx;
  margin: 24rpx 0;
  box-sizing: border-box;
}
.muted {
  display: block;
  color: #6b7280;
  font-size: 24rpx;
  margin: 12rpx 0;
}
.body {
  display: block;
  white-space: pre-wrap;
  word-break: break-word;
  line-height: 1.8;
  margin: 24rpx 0;
}
.row {
  display: flex;
  align-items: center;
  gap: 12rpx;
  flex-wrap: wrap;
}
.spread {
  justify-content: space-between;
}
.grow {
  flex: 1;
}
.input {
  background: #f3f5f8;
  border-radius: 12rpx;
  padding: 20rpx;
  margin: 18rpx 0;
  box-sizing: border-box;
  min-height: 80rpx;
  width: 100%;
}
.editor {
  height: 360rpx;
}
.primary {
  background: var(--primary);
  color: #fff;
  margin-top: 24rpx;
}
button {
  font-size: 28rpx;
  border-radius: 12rpx;
}
button[size='mini'] {
  font-size: 24rpx;
  margin: 8rpx 0;
}
.selected {
  color: var(--primary);
}
.status {
  text-align: center;
  padding: 28rpx;
  color: #6b7280;
}
.error {
  color: #bd303c;
  display: block;
}
.post-image {
  display: block;
  width: 100%;
  border-radius: 14rpx;
  margin: 12rpx 0;
}
.image-placeholder {
  background: #eef1f5;
  padding: 24rpx;
  text-align: center;
  color: #6b7280;
}
.waterfall {
  display: flex;
  flex-wrap: wrap;
  justify-content: space-between;
  align-items: flex-start;
}
.post-card {
  width: 48%;
  padding: 16rpx;
  margin: 12rpx 0;
}
.summary {
  display: -webkit-box;
  -webkit-line-clamp: 3;
  -webkit-box-orient: vertical;
  overflow: hidden;
  line-height: 1.6;
}
.image-grid {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: 14rpx;
}
.tag {
  color: var(--primary);
}
.comment {
  padding: 20rpx 0;
  border-bottom: 1rpx solid #eee;
}
.menu button {
  background: #fff;
  text-align: left;
  border-bottom: 1rpx solid #eef1f5;
  border-radius: 0;
}
</style>
