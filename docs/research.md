# 主流剪贴板管理器功能调研

> 调研时间：2026-11-25
> 参考对象：Ditto、CopyQ、EcoPaste、Windows 11 内置剪贴板（Win+V）、Maccy / Paste 类产品

## 一、四款代表产品的定位

| 产品 | 平台 | 技术栈 | 定位 | 亮点 |
|---|---|---|---|---|
| **Ditto** | Windows | C++ / Win32 + SQLite | Windows 老牌全能型 | 无限历史、Quick Paste 数字槽、分组、局域网加密同步、OCR |
| **CopyQ** | Win/mac/Linux | Qt / C++ | 极客型，可编程 | 多标签页、标签+备注、脚本 API、命令行、忽略规则 |
| **EcoPaste** | Win/mac | Tauri v2 + Rust + React | 现代轻量本地优先 | SQLite FTS5 搜索、按来源应用/类型筛选、敏感信息跳过、加密备份、拖出 |
| **Win11 内置** | Windows | 系统组件 | 最低配基线 | 25 条上限、置顶、跨设备同步（仅文本）、清空 |

结论：**"轻量"和"全功能"不冲突**——EcoPaste 证明了 Tauri 架构能把安装包压到 10MB 级别，同时覆盖 90% 的高级功能。

## 二、功能清单（按成熟度分层）

### 1. 基础必备（缺了就不算剪贴板管理器）

- 后台常驻、自动监听剪贴板变化
- 历史列表（可配置保留条数 / 天数）
- 全局热键唤出面板（如 Ctrl+`、Alt+V）
- 即时搜索过滤
- 键盘操作：↑↓ 选择、Enter 粘贴、Esc 关闭
- 粘贴到"上一个前台窗口"（自动还原焦点 + 模拟 Ctrl+V）
- 置顶 / 收藏（Pin）
- 单条删除、全部清空
- 连续重复内容去重
- 数据持久化（重启不丢）
- 托盘图标、开机自启、低占用

### 2. 内容类型支持

- 纯文本（CF_UNICODETEXT）
- 富文本（HTML / RTF），且**回贴时多格式一起写回**，否则粘到 Word 掉格式
- 图片（PNG / DIB / DIBV5），缩略图 + 原图落盘
- 文件 / 文件夹（CF_HDROP）——Win11 内置不支持，是差异化重点
- 混合格式条目（同一份内容同时含 text + html + rtf）

### 3. 组织与检索（Ditto / CopyQ / EcoPaste 都有）

- 分组 / 标签 / 分类页（CopyQ 的 tab，Ditto 的 group）
- 备注（note）
- 收藏夹（favorite）与置顶（pin）分离
- 按类型筛选：文本 / 图片 / 文件 / 链接 / 颜色 / 代码
- 按来源应用筛选，显示来源应用图标
- 搜索增强：模糊匹配、拼音首字母、FTS5 全文索引
- 条目内编辑后再粘贴
- 拖拽：从列表拖到其他应用 / 从外部拖入

### 4. 粘贴体验（决定"顺手"程度）

- 快捷槽位：数字键 1-9 直接粘第 N 条（Ditto Quick Paste）
- 纯文本粘贴（去除格式，Shift+Enter）
- 粘贴后：保留剪贴板 / 恢复原剪贴板 / 自动隐藏窗口，均可配置
- 顺序粘贴（Paste Stack）：连续 Ctrl+V 依次吐出自选的一组条目
- 文本转换动作：大小写、去空行、JSON 美化、URL 编码、Markdown→纯文本
- 智能识别：URL 可点击打开、颜色值可预览、路径可定位

### 5. 隐私与安全（成熟产品的分水岭）

- 按进程名排除（密码管理器：KeePass / 1Password / Bitwarden 等）
- 按内容规则忽略（正则、超长文本、特定格式）
- 一键暂停监听（临时 / 定时）
- 敏感内容识别：私钥、AWS Key、JWT、信用卡号、高熵字符串 → 跳过或打码
- 数据库加密（SQLCipher / 自加密备份）
- 退出时自动清空

### 6. 进阶 / 加分项

- 文本片段（Snippets）+ 变量占位符（日期、剪贴板、光标位置）
- OCR 图片文字（Ditto 有）
- 多设备同步：局域网 P2P 加密 / WebDAV / 自建服务
- 命令行 / HTTP API / 脚本扩展（CopyQ 的核心竞争力）
- 便携模式（配置随 exe 走，不写注册表）
- 备份 / 恢复（加密导出 .bak）
- 自动更新
- 使用统计（复制次数、最常用条目）
- 多语言、深色/浅色、Mica 亚克力、高 DPI、多屏定位

## 三、Windows 平台实现要点（踩坑清单）

1. **监听方式**：`AddClipboardFormatListener` + 处理 `WM_CLIPBOARDUPDATE`，事件驱动、空闲零 CPU。不要轮询。
2. **读剪贴板**：`OleGetClipboard` 拿 `IDataObject`，枚举格式；注意延迟渲染（Delayed Rendering）要先 `GetData` 才拿到真数据。
3. **格式细节**：
   - `CF_HTML` 带 `Version / StartHTML / EndHTML / StartFragment / EndFragment` 头，手写时必须算偏移，否则浏览器/Word 不认。
   - 图片优先取 `PNG` 自定义格式（保真+带透明），退化到 `CF_DIBV5`。
   - 文件取 `CF_HDROP`，用 `DragQueryFile` 枚举。
4. **写回多格式**：一份内容要同时写 text + html + rtf + 图片，粘贴目标应用各取所需。
5. **模拟粘贴**：记录前台窗口 → `SetForegroundWindow` → `SendInput` 发 Ctrl+V。注意 UIPI：**普通权限进程无法向管理员权限窗口注入输入**，必要时提示用户以管理员运行。
6. **面板不抢焦点**：`WS_EX_NOACTIVATE` + 低级键盘钩子，或在唤出后接管焦点、Esc 时把焦点还给原窗口。
7. **全局热键**：`RegisterHotKey`，要做冲突检测和自定义；可选支持"双击 Ctrl"这类低级钩子触发。
8. **存储**：SQLite（WAL 模式 + FTS5），二进制（图片/文件快照）落盘单独目录，用 SHA-256 去重。
9. **大对象保护**：超过阈值（如文本 1MB / 图片 20MB）只存摘要或不存，防止内存爆掉。
10. **单实例**：命名互斥体，避免双开抢监听。
11. **性能**：列表虚拟滚动、缩略图懒加载、写入防抖/批量事务；1000 条历史唤出仍要 <100ms。
12. **异常**：剪贴板被其他程序占用时 `OpenClipboard` 会失败，需要重试（10~50ms 退避）。

## 四、参考来源

- CopyQ 官方站与文档：多标签、标签备注、脚本 API、忽略规则
- Ditto 项目页与下载页：Quick Paste、分组、加密局域网同步、OCR
- EcoPaste README：Tauri v2 + Rust + React、FTS5 搜索、敏感信息跳过、加密备份
- Microsoft 支持文档：Win+V 仅保留 25 条、仅文本可跨设备同步
