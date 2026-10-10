# 连接分组管理视觉检查

- Source visual truth: `/var/folders/qv/hwqb97ld5lv__6ykms3_yrgc0000gn/T/codex-clipboard-44a0ea40-cab3-4546-ae84-1f99caa15684.png`
- Implementation screenshot: `/tmp/superlink-groups-qa/connection-groups.png`
- Viewport: 1470×862，原生 Fyne 测试画布。两张图片均为 1470×862；按 1:1 像素比较，不使用 CSS。
- State: 浅色、管理窗口打开、未分组、5 个测试连接、无勾选。
- Comparison evidence: 在同一工具结果中打开参考与实现，比较完整画面及模态区域。窗口约 980×630，与参考一致；无需额外裁切。

## 检查结果

- 字体：使用项目现有中文字体，标题层级、单行截断、时间和按钮文字可读。原生字体与参考 Web 字体存在字重差异，列为 P3。
- 布局：保留左右两栏、左侧分组计数、右侧排序、连接表及行操作；四角圆润。批量移动操作和反馈位于底部，是功能适配的明确增补。
- 颜色：绿色创建按钮、浅绿选中行、灰白背景、红色删除图标；支持项目深色模式颜色。
- 资产：使用项目现有图标资源；没有替换或新增图片资产需求。
- 内容：保持 SuperLink 品牌，展示真实本地连接；测试截图不复制参考中的地址与用户数据。

## 比较迭代

1. 发现 P1：自定义单元格未渲染；改为 Fyne BaseWidget。发现 P2：异步加载后表格高度未更新；刷新表格宿主布局。
2. 发现 P2：蓝色创建按钮、删除按钮大红底、数量紧贴名称、左栏触及外框下角；调整主题、红色线形图标、计数右对齐和内容边距。
3. 最终同视口截图中表头、5 行连接、全部操作按钮、圆角及分组计数正常，无待处理 P0/P1/P2。

## 功能验证

原生测试通过：持久分组、排序、批量移动、全选、行绑定、确认删除、空状态。应用层验证时间保留及冲突事务回滚。完整 UI、application、state 测试及 go vet 通过。遵循用户要求未使用浏览器；未对真实用户连接执行删除或移动。

final result: passed

## 新建分组补充检查

- Source: `/var/folders/qv/hwqb97ld5lv__6ykms3_yrgc0000gn/T/codex-clipboard-b1b1a390-0994-41f2-bf3a-bc9d32999f00.png`（538×501）
- Native implementation: `/tmp/superlink-group-form/new-connection-group.png`（1000×700）
- 同一工具结果中打开两图，比较模态内容区域：参考约 (11,7) 520×480；原生约 (240,110) 520×480，均按 1:1 像素，不比较外围画布。
- 保留绿色文件夹标识、标题/说明、必填标记、三个表单区、父分组提示、两行连接、边框卡片、圆角和右下按钮。
- 修正父分组下拉边框、图标颜色、列表宽度和反馈区域。连接列表依据实际最长名称自适应，字段首次自动聚焦；名称为空时按钮禁用是功能状态差异。
- 原生字体字重与参考略有区别（P3）；无需新增图片资产。未使用浏览器。
- 功能测试覆盖创建/勾选/父层级/重复反馈/取消，应用层事务测试覆盖冲突回滚。无剩余 P0/P1/P2。

final result: passed

## 新建分组 HTML 样式补充检查

- 最新参考：`/Users/bre/.codex/attachments/ff887f21-fadd-4ffb-8d62-60eea6430df6/已粘贴的文本.txt`，直接读取 HTML/CSS；未执行其中的脚本。
- 原生截图：`/tmp/superlink-group-form-html/new-connection-group.png`（1000×800）；小窗口截图 `new-connection-group-compact.png`（700×520）。
- 检查名称/父级/六色/描述/默认开关的顺序，460px 宽、20px 外角、44px 输入高度、40px 底部按钮、24px 内边距与蓝紫配色。
- 修正原生 SVG 圆角绘制带来的色块外角、缺失图标回退为文档图标、下拉控件无边框、数据库标识颜色。使用本地原生圆角渐变画布及已有图标。
- 小窗口的表单可滚动，底部取消/创建按钮保持可见；不为保存后的默认分组迁移既有连接。
- 无 P0/P1/P2 视觉阻塞。字体字重与浏览器呈现可能不同（P3），未进行或宣称浏览器像素对比。

final result: passed (native visual inspection)

## 父级分组下拉补充检查

用户指出默认下拉菜单行高、方角和整块蓝色焦点背景不协调。原生截图 `/tmp/superlink-group-dropdown/parent-group-dropdown.png`（1000×800）和 `parent-group-dropdown-compact.png`（700×520）显示替换后的 12px 圆角同宽浮层、40px 选项行、浅蓝选中背景和蓝色勾号。输入保持白底，聚焦用边框。长列表控制高度且可滚动，浮层保持在画布内。鼠标和键盘选值、Escape/外部取消、失焦关闭和禁用通过原生测试。未使用浏览器，未宣称浏览器像素对比。

final result: passed (native visual inspection)

## 父级菜单最终截图修正

- 最终视觉依据：`/var/folders/qv/hwqb97ld5lv__6ykms3_yrgc0000gn/T/codex-clipboard-e8f4a095-347f-4632-8ec0-4431e8c3cb10.png`。
- 实现：`/tmp/superlink-group-menu-native-style/parent-group-dropdown.png`。将实现截图和参考在同一结果中打开，比较菜单区域；参考宽约 410px，实现宽约 414px，均有四项、24px 行高和约 104px 总高度。
- 当前项改为蓝底白字、左侧勾号；移除文件夹和右侧勾号。菜单覆盖控件，输入框右侧箭头保留。底部可见来自下方标识色的轻微模糊色彩，外角和内角一致。
- 使用原生 Fyne 模糊绘制，没有调用浏览器。字体仍使用项目字体，不宣称是 AppKit 系统菜单。无 P0/P1/P2 视觉阻塞。

final result: passed (native screenshot comparison)
