export function ShareWorkspace() {
  return <section className="share-workspace">
    <p className="eyebrow">人工分享 / MANUAL SHARE</p>
    <h1>把校园内容<br />带到当前群聊</h1>
    <p className="share-intro">选择已审核内容，预览模板，在群内确认后发送。</p>
    <div className="share-notice" role="status">
      <strong>分享功能尚未开放</strong>
      <p>学校授权与企业微信身份接入完成后，可在当前客户群内使用此入口。</p>
    </div>
    <ol className="share-steps">
      <li><span>01</span><div><strong>确认学校与群聊</strong><p>根据成员权限和当前群上下文核对目标。</p></div></li>
      <li><span>02</span><div><strong>选择内容与模板</strong><p>只选择已审核、仍可分享的校园内容。</p></div></li>
      <li><span>03</span><div><strong>预览并人工确认</strong><p>核对完整正文，再执行分享操作。</p></div></li>
    </ol>
  </section>
}
