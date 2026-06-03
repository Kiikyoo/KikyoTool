# KikyoTool

KikyoTool 是一个 Rust 桌面 GUI，用来做常见 Android 自动化操作：

- 启动 ADB 并刷新设备列表。
- 支持 USB 设备和 TCP/IP 设备连接。
- 一键获取手机应用包名，支持第三方、全部、系统应用范围。
- 选择 APK 后执行 `adb install`。
- 预留 Frida 自动化命令模板，方便未来扩展。

## 环境要求

- Android platform-tools：`adb` 需要在界面里填写 adb 的完整路径。

## 使用提示

- 设备显示 `unauthorized` 时，需要在手机上确认 USB 调试授权弹窗，然后刷新设备。
- `adb install` 支持界面勾选覆盖安装 `-r`、允许降级 `-d`、授予运行时权限 `-g`。
- Frida 区域目前提供复制命令模板，后续会继续完善拓展。
