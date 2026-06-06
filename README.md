# Codex Account Manager

管理多个 Codex 账号，一键切换 `~/.codex/auth.json`。

## 环境要求

- [Node.js](https://nodejs.org/) >= 18
- [Rust](https://www.rust-lang.org/tools/install) >= 1.77
- macOS 或 Windows

## 安装依赖

```bash
npm install
```

## 开发模式运行

```bash
npm run tauri dev
```

## 打包 macOS 程序

```bash
npm run tauri build
```

打包完成后，可执行文件位于：

- **App**: `src-tauri/target/release/bundle/macos/Codex Account Manager.app`
- **DMG**: `src-tauri/target/release/bundle/dmg/Codex Account Manager_0.1.0_aarch64.dmg`

直接双击 `.app` 即可运行，或将 `.dmg` 拖到 Applications 文件夹安装。

## 打包 Windows 程序

```bash
npm run tauri build
```

打包完成后，安装包位于：

- **MSI**: `src-tauri/target/release/bundle/msi/`
- **EXE**: `src-tauri/target/release/bundle/nsis/`

## 功能说明

- **添加账号**: 输入账号名、开通时间、auth.json 的完整 JSON
- **账号列表**: 显示账号名、类型(Free/Plus/Pro)、5小时额度、周额度、刷新时间
- **当前账号**: 自动识别当前使用的账号，绿色高亮显示
- **一键切换**: 点击运行按钮，自动杀掉 Codex 进程 → 写入 auth.json → 重启 Codex
- **额度刷新**: 通过 ChatGPT API 自动获取额度信息，支持定时刷新(5/10/15/30分钟)
