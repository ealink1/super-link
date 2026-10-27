# SuperLink 普通 AI 问答

用户要求：点击右上角 AI 入口，在右侧展示普通问答，参考提供的 SuperLink 截图。

## 界面与范围

- 沿用右上角灯泡入口，开关右侧约 380 点宽的聊天面板。
- SQL / Shell / Note 共用面板和日夜主题；切换工作区保留对话及未发送输入。
- 标题 SuperLink AI，新对话、设置、关闭；中部消息滚动区；底部多行输入、发送 / 停止、状态。
- 支持多轮文字问答和安全 Markdown 展示、复制回复。关闭面板仅隐藏，退出应用取消并等待网络请求。
- 不增加数据库自动执行、自动上下文、Agent、图片附件。仅发送用户输入及当前成功对话历史。聊天不落盘。

## 服务与配置

首版使用 OpenAI 兼容 Chat Completions，可配置 API 基础地址、Key、模型及流式开关。Ollama 可使用其兼容 `/v1` 地址。未配置时展示引导，保存配置不自动发送请求。

配置通过现有本机 AES-GCM Vault 加密；SQLite 仅保存不透明引用。替换配置先持久化新密文和引用，再删除旧密文。密码不进入日志或远端错误文案。

UI → application.AISettings → state / secrets；UI → infra/chat.Client → HTTP。应用层和协议层不依赖 Fyne。

HTTP 请求有取消及总体超时；禁止重定向，远端使用 HTTPS，HTTP 仅允许 localhost / 私有 IP 供本地服务。校验地址、消息和响应尺寸，流式事件也有上限。网络错误只展示本地固定文案，不直接输出远端响应。

## 验收

httptest 覆盖流式 / 普通响应、多轮请求、取消、状态错误、重定向、响应上限；加密配置覆盖重启读取及保存失败恢复。Fyne 测试覆盖右侧布局、跨工作区状态、停止 / 清空、主题与迟到回调。原生隔离测试不使用生产配置或付费 API。执行相关竞态检查、静态检查及 macOS 原生构建；不使用浏览器测试。

协议参考：[OpenAI Chat API](https://developers.openai.com/api/reference/resources/chat)、[Ollama OpenAI compatibility](https://docs.ollama.com/api/openai-compatibility)。
