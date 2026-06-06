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

## 数据保存位置

账号列表保存在本机 SQLite 数据库 `codex_accounts.db` 中，应用首页会直接显示当前机器的完整路径。

常见默认位置：

- macOS: `~/Library/Application Support/com.codex.account-manager/codex_accounts.db`
- Windows: `%APPDATA%\com.codex.account-manager\codex_accounts.db`

当前 Codex 正在使用的登录信息仍然是官方位置：

- macOS / Linux: `~/.codex/auth.json`
- Windows: `%USERPROFILE%\.codex\auth.json`

注意区别：

- `codex_accounts.db` 是本工具管理的账号仓库，里面保存多个账号。
- `auth.json` 是当前生效账号，点击「运行」时会由本工具写入。

## 换电脑迁移

1. 在旧电脑打开应用，复制首页显示的「账号库」路径。
2. 退出应用，复制 `codex_accounts.db` 文件。
3. 在新电脑安装并运行一次应用，让它创建应用数据目录。
4. 退出应用，把旧电脑的 `codex_accounts.db` 覆盖到新电脑相同位置。
5. 重新打开应用，账号列表会恢复；需要使用哪个账号，再点击对应账号的「运行」。

`codex_accounts.db` 和 `auth.json` 都包含敏感 token，请不要提交到 Git，也不要发给别人。
