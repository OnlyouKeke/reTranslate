# reTranslate

<p align="center">
  <strong>基于 GPUI 的高性能跨平台原生桌面 AI 翻译阅读器</strong><br>
  <em>A High-Performance Native Desktop AI Translation Reader powered by GPUI (Rust)</em>
</p>

---

## 🌟 特性概览 / Key Features

- **原生极致性能 (Native & Lightweight)**: 基于 Rust 与 GPUI 原生构建，告别 Chromium/Electron 庞大内存占用，丝滑 120 FPS 渲染。
- **全格式文献支持 (Comprehensive Format Parsing)**:
  - 📕 **PDF**: 采用纯 Rust `lopdf` 实现纯净流式段落重排与原版页面平铺双对照模式。
  - 📘 **EPUB**: 完整支持 EPUB 2/3 标准、NCX/Nav 目录抽取与 XHTML 安全无损解析。
  - 📝 **DOCX**: 原生解析 Word 文档结构、标题层级与复杂嵌套合并单元格表格。
  - 📄 **Markdown**: GFM 语法高亮兼容、嵌套列表深度还原与表格排版。
  - 📑 **TXT**: 智能 BOM 探测、UTF-8 / GB18030 / CJK 软换行智能拼合与全自动章节划分。
- **OpenAI 兼容 AI 翻译核心 (OpenAI-Compatible Engine)**:
  - 零配置硬编码绑定：首次启动引导向导，按需选择 DeepSeek、OpenAI、Ollama、SiliconFlow 或任意自定义端点。
  - 深度思考过滤：内置有限状态机流式过滤 `<think>...</think>` 推理内容。
  - 编号批量翻译协议：有效节约 60%+ Token 消耗与网络握手往返。
- **沉浸式双语对照与多阅读模式 (Immersive Reading Modes)**:
  - **行内柔和 (Subtle Muted)**: 灰色次级文字紧随原文，阅读不被打断。
  - **行内虚线 (Dotted Divider)**: 优雅虚线分隔原文字与译文。
  - **左右双栏 (Side-by-Side)**: 经典学术文献双栏平行排版。
  - **纯翻译视图 (Target Only)**: 一键沉浸阅读纯译文版。
  - **纯原文视图 (Original Only)**: 无干扰原文阅读。
- **版式与交互设置 (Layout & Ergonomics)**:
  - 页面阅读宽度自由调节：窄屏 680px、标准 850px、宽屏 1100px、全宽 100%。
  - **按住空格键拖动 (Space + Drag)**: 抓手手形工具平滑平移画布。
  - 键盘快捷键导航：`Space` (下一页)、`Shift+Space` (上一页)、`[` / `]` (章节跳转)、`Alt+A` (快速翻译当前视口)。
- **严密的用户态错误隔离 (Strict Error Isolation)**:
  - 用户界面中翻译发生异常时，永远仅显示友好明确的 **「翻译失败，请稍后重试」** 并附带 **「重试」** 按钮。
  - HTTP 状态码、详细调用堆栈、网络延迟与敏感 API Key 自动脱敏并隔离写入内部日志。
- **全能导出 (Versatile Exporting)**:
  - 支持将阅读内容导出为 **EPUB / DOCX / Markdown / HTML**。
  - 导出可选 **双语对照** 或 **纯译文**。

---

## ⌨️ 常用快捷键 / Keyboard Shortcuts

| 快捷键 | 功能描述 (Function) |
| :--- | :--- |
| `Space + 鼠标左键拖拽` | **手形抓手工具**：在阅读画布上平滑自由拖动页面 (Hand tool pan) |
| `Space` | 向下滚动翻页 (Scroll page down) |
| `Shift + Space` | 向上滚动翻页 (Scroll page up) |
| `]` | 跳转下一章节 (Next chapter) |
| `[` | 跳转上一章节 (Previous chapter) |
| `Alt + A` | 翻译当前章节/当前视口未翻译段落 (Translate chapter) |
| `Ctrl + E` / `Cmd + E` | 打开导出对话框 (Export document) |
| `Ctrl + ,` / `Cmd + ,` | 打开 API 接口与首选项设置 (Preferences) |

---

## 🚀 编译与本地运行 / Build & Development

### 环境需求 (Prerequisites)
- [Rust 1.80+](https://rustup.rs/) (edition 2021)
- **Linux 系统依赖** (如 Ubuntu/Debian):
  ```bash
  sudo apt-get update
  sudo apt-get install -y libasound2-dev libx11-dev libxcursor-dev libxi-dev \
      libxinerama-dev libxrandr-dev libxkbcommon-dev libwayland-dev pkg-config
  ```

### 运行单元与集成测试
```bash
cargo test --tests
```

### 本地编译与启动
```bash
cargo run --bin retranslate
```

---

## 📦 打包与发布 (Packaging & Releases)

项目预置了全自动 GitHub Actions CI/CD 多平台构建流水线：

### Windows 打包 (MSI 安装包与免安装便携版)
- 借助 WiX Toolset 构建原生 Windows 安装包：
  ```powershell
  cargo build --release --bin retranslate
  candle.exe -arch x64 -ext WixUIExtension -out target/main.wixobj wix/main.wxs
  light.exe -ext WixUIExtension -out dist/reTranslate-windows-x64.msi target/main.wixobj
  ```
- 包含开始菜单快捷方式、桌面快捷方式与干净卸载逻辑。

### macOS / Linux 打包
- macOS: 原生二进制便携 `.tar.gz` 及 `.dmg` 安装映像。
- Linux: 便携式 `.tar.gz` 发布包。

---

## 📄 开源许可证 / License

本项目采用 MIT 许可证开源。详情请参阅 [LICENSE](LICENSE)。
