# Codex Account Manager

管理多个 Codex 账号，一键切换 `~/.codex/auth.json`。

## 环境要求

- [Node.js](https://nodejs.org/) >= 20.19 或 >= 22.12
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
- **一键切换**: 点击运行按钮，写入 auth.json；可选择是否自动重启 Codex
- **额度刷新**: 通过 ChatGPT API 自动获取额度信息，支持定时刷新(5/10/15/30分钟)
- **本地保存**: auth.json 内容保存到本机 SQLite 账号库，刷新和切换不再访问系统钥匙串
- **加密备份**: 支持用密码导出/导入加密备份，方便换电脑迁移

## 数据保存位置

账号列表保存在本机 SQLite 数据库 `codex_accounts.db` 中，应用首页提供「打开账号库目录」和「详情」入口。

常见默认位置：

- macOS: `~/Library/Application Support/com.codex.account-manager/codex_accounts.db`
- Windows: `%APPDATA%\com.codex.account-manager\codex_accounts.db`

当前 Codex 正在使用的登录信息仍然是官方位置：

- macOS / Linux: `~/.codex/auth.json`
- Windows: `%USERPROFILE%\.codex\auth.json`

注意区别：

- `codex_accounts.db` 是本工具管理的本地账号库。
- 每个账号的 auth.json 敏感内容会保存在 SQLite 中，适合个人本机使用，请不要外传数据库。
- `auth.json` 是当前生效账号，点击「运行」时会由本工具写入。

## 换电脑迁移

推荐使用应用内的加密备份功能：

1. 在旧电脑打开应用，点击「导出加密备份」。
2. 设置至少 8 位备份密码，并保存导出的 `.json` 文件。
3. 在新电脑安装并打开应用，点击「导入备份」。
4. 选择备份文件并输入同一个密码，账号会恢复到新电脑的本地账号库。
5. 需要使用哪个账号，再点击对应账号的「运行」。

备份文件和 `auth.json` 都包含敏感 token。请妥善保管备份密码，不要把备份文件提交到 Git，也不要发给别人。

可以复制 `codex_accounts.db` 迁移，但它包含完整 auth.json/token。更推荐使用加密备份，避免数据库明文外泄。
