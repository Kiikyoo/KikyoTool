# KikyoTool

KikyoTool 是一个 Rust 桌面 GUI，用来做常见 Android 自动化操作：

- 启动 ADB 并刷新设备列表。
- 支持 USB 设备和 TCP/IP 设备连接。
- 一键获取手机应用包名，支持第三方、全部、系统应用范围。
- 选择 APK 后执行 `adb install`。
- 预留 Frida 自动化命令模板，方便未来扩展。

## 环境要求

- Rust 工具链：<https://www.rust-lang.org/tools/install>
- Android platform-tools：`adb` 需要在 `PATH` 中，或者在界面里填写 adb 的完整路径。
- Frida 面板是预留扩展区，可选安装：
  - `pip install frida-tools`
  - 确保 `frida`、`frida-ps`、`frida-trace` 可执行文件在 `PATH` 中。
- 程序会自动尝试加载 Windows 中文字体，如 `Deng.ttf`、`simhei.ttf`，用于避免 `egui` 默认字体导致中文乱码。

## 运行

```powershell
cargo run --release
```

## 使用提示

- 设备显示 `unauthorized` 时，需要在手机上确认 USB 调试授权弹窗，然后刷新设备。
- `adb install` 支持界面勾选覆盖安装 `-r`、允许降级 `-d`、授予运行时权限 `-g`。
- Frida 区域目前提供命令模板和 `frida-ps -Uai` 执行入口，后续可以在 `src/frida.rs` 中继续扩展自动化流程。
