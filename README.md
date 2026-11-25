# ClipNest · 轻量全功能剪贴板

Windows 上的本地剪贴板管理器。常驻托盘，一个热键唤出，文本 / 图片 / 文件全都能翻回去。

> 技术栈：Tauri 2 + Rust + React + SQLite（全部本地存储，不联网）

## 已实现

**捕获**
- 事件驱动监听（`AddClipboardFormatListener`，空闲 0% CPU）
- 纯文本、HTML/RTF 富文本、图片（PNG / CF_DIB / CF_DIBV5）、文件与文件夹
- 内容哈希去重，重复复制只累加次数并置顶

**面板**
- 全局热键唤出（默认 `Alt+V`），出现在鼠标附近，失焦自动隐藏
- 即时搜索、类型筛选（全部 / 文本 / 图片 / 文件 / 收藏）
- 键盘全流程：↑↓ 选择、Enter 粘贴、Shift+Enter 纯文本、Ctrl+P 置顶、Del 删除、Esc 关闭、1-9 快捷粘贴
- 右侧预览：文本全文、图片大图、文件路径列表
- 置顶 / 收藏 / 仅复制 / 直接粘贴

**粘贴**
- 记住唤出前的窗口，粘贴时自动还原焦点并发送 Ctrl+V
- 回写多格式（文本 + HTML + RTF + PNG + DIB），粘到 Word / 浏览器不掉格式
- 粘贴后隐藏面板（可配置）

**其它**
- 托盘菜单：打开面板 / 设置 / 暂停记录 / 清空历史 / 退出
- 开机自启（HKCU Run）
- 保留策略：按条数 / 按天数自动清理，置顶与收藏不受影响
- 单实例、高 DPI、深色面板

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
| 唤出 / 隐藏面板 | `Alt+V`（可在设置中改） |
| 选择 | `↑` `↓` |
| 粘贴 | `Enter` |
| 纯文本粘贴 | `Shift+Enter` |
| 置顶 / 取消 | `Ctrl+P` |
| 删除当前条 | `Delete` |
| 快速粘贴前 9 条 | `1`~`9` |
| 关闭面板 | `Esc` |

## 已知限制

- 无法向**以管理员身份运行**的窗口发送 Ctrl+V（Windows UIPI 限制），这类窗口里请用"纯文本/复制"按钮手动粘贴。
