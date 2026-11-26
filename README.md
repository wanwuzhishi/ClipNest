# ClipNest · 轻量全功能剪贴板

Windows 上的本地剪贴板管理器。常驻托盘，一个热键唤出，文本 / 图片 / 文件全都能翻回去。

> 技术栈：Tauri 2 + Rust + React + SQLite（全部本地存储，不联网）

## 已实现

**捕获**
- 事件驱动监听（`AddClipboardFormatListener`，空闲 0% CPU）
- 纯文本、HTML/RTF 富文本、图片（PNG / CF_DIB / CF_DIBV5）、文件与文件夹
- 内容哈希去重，重复复制只累加次数并置顶

**面板**
- 全局热键唤出（默认 `Alt+V`），出现在鼠标附近
- **唤出时不抢焦点**：窗口用 `WS_EX_NOACTIVATE` 显示，资源管理器搜索框、浏览器地址栏这类"失焦就关掉"的输入位置不会被破坏
- 未激活状态下用全局低级键盘钩子接管导航键；点搜索框才进入输入模式
- 即时搜索、类型筛选（全部 / 文本 / 图片 / 文件 / 收藏）
- 键盘全流程：↑↓ 选择、Enter 粘贴、Shift+Enter 纯文本、Ctrl+P 置顶、Del 删除、Esc 关闭、1-9 快捷粘贴
- 面板默认 410px 宽（单列列表）；点标签栏最右侧的方框图标展开预览，窗口宽度翻倍到 820px，左边缘保持不动、超出屏幕会自动左移
- 预览支持**编辑文本**：点「编辑」→ 改完点「保存」直接写回数据库（只对文本类型）
- 预览区：文本全文、图片大图、文件路径列表
- 列表项右侧悬停出现图钉 / 星标按钮，收起预览时也能直接置顶和收藏（已选中的常显）
- 置顶 / 收藏 / 仅复制 / 直接粘贴
- **面板固定**：右上角图钉（或 `Ctrl+L`）钉住后，粘贴完不自动关闭，且每 250ms 重新置顶，始终压在最前
- 设置里可开「单击条目直接粘贴」，不开时单击只选中、回车才粘贴

**粘贴**
- 记住唤出前的窗口和**具体焦点控件**（`GetGUIThreadInfo`），粘贴时精确还原
- 按需等待：焦点不用切换时只等 6ms 就发 Ctrl+V，需要切窗口才等 45ms
- 资源管理器搜索框这类 DirectUI 容器只还原窗口不抢控件焦点，避免把搜索框弄丢
- 回写多格式（文本 + HTML + RTF + PNG + DIB），粘到 Word / 浏览器不掉格式
- 粘贴后隐藏面板（可配置）

**隐私**
- 进程排除名单：密码管理器（KeePass / 1Password / Bitwarden 等默认内置）复制的内不记录
- 敏感内容识别：私钥、AWS/GitHub/Slack 令牌、JWT、信用卡号自动跳过
- 尊重剪贴板忽略标记（`ExcludeClipboardContentFromMonitorProcessing` / `Clipboard Viewer Ignore`）
- 每条记录保存来源进程名

**其它**
- 托盘菜单：打开面板 / 设置 / 暂停记录 / 清空历史 / 退出
- 开机自启（HKCU Run）
- 保留策略：按条数 / 按天数自动清理，置顶与收藏不受影响
- 单实例、高 DPI、深色面板
- 圆角用 Win11 的 DWM 原生圆角（`DWMWA_WINDOW_CORNER_PREFERENCE`），带抗锯齿

## 开发

```bash
npm install
npm run app:dev      # 开发模式（热重载）
npm run app:build    # 打安装包
```

单独构建：

```bash
npm run build                      # 前端 → dist/
cargo build --release --manifest-path src-tauri/Cargo.toml
```

产物：`src-tauri/target/release/clipnest.exe`

## 数据位置

```
%APPDATA%\com.alma.clipnest\
├─ clipnest.db      历史记录（SQLite，WAL）
├─ media\           图片原图
└─ settings.json    设置
```

## 目录结构

```
src/                 React 前端（面板 + 设置页）
src-tauri/src/
├─ lib.rs            启动、托盘、热键、窗口定位
├─ clipboard.rs      Win32 剪贴板读写与监听
├─ paste.rs          焦点恢复与 Ctrl+V 模拟
├─ store.rs          SQLite 数据层
├─ settings.rs       配置与开机自启
└─ commands.rs       前后端命令
docs/research.md     竞品功能调研
PLAN.md              分阶段实施清单
```

## 快捷键

| 操作 | 按键 |
|---|---|
| 唤出 / 隐藏面板 | `Alt+V`（可在设置中改，不抢焦点） |
| 输入搜索 | 点击搜索框（会激活面板） |
| 选择 | `↑` `↓` |
| 粘贴 | `Enter` |
| 纯文本粘贴 | `Shift+Enter` |
| 置顶 / 取消（该条目） | `Ctrl+P` |
| 固定面板 / 取消（粘贴后不关） | `Ctrl+L` 或点右上角图钉 |
| 删除当前条 | `Delete` |
| 快速粘贴前 9 条 | `1`~`9` |
| 关闭面板 | `Esc` 或右上角 × |

## 已知限制

- 无法向**以管理员身份运行**的窗口发送 Ctrl+V（Windows UIPI 限制），这类窗口里请用"纯文本/复制"按钮手动粘贴。

## 设计说明：为什么不抢焦点

常见剪贴板工具的唤出方式是把面板激活到前台，这会让资源管理器搜索框、各种"失焦即取消"的输入框直接关闭，粘贴目标就没了。

ClipNest 的做法是面板 **永不激活**（`WS_EX_NOACTIVATE`），前台窗口和焦点控件从头到尾不变：

```
Alt+V  →  SetWindowPos(SWP_NOACTIVATE)  →  面板显示，焦点仍在原输入框
↑↓     →  WH_KEYBOARD_LL 钩子捕获，转发给前端
Enter  →  写剪贴板 → 隐藏面板 → 还原目标控件焦点 → Ctrl+V
```

只有在点击搜索框时才显式激活面板进入输入模式——这一步用户是主动的，可以接受原输入框被关闭。
