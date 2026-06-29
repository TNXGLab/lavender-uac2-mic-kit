# lavender UAC2 USB Microphone

把 Redmi Note 7 / Redmi Note 7 Pro 国际常见代号 `lavender` 变成电脑可识别的实体 USB 麦克风。

它不是虚拟声卡。手机通过 USB Gadget 暴露 UAC2 麦克风设备，Android 端用 AAudio 读取手机实体麦克风，再用 Rust/RNNoise 做降噪并送到 USB 输入端。

## 适用范围

1.  设备：小米 Redmi Note 7，设备代号必须是 `lavender`。
2.  系统环境：需要能进 fastboot，并且能使用 `adb`。
3.  Root 环境：发布包里的 boot 已包含 KernelSU，用于开机自动启动 USB 麦克风服务。
4.  电脑端：macOS 已验证，理论上 Linux/Windows 也会识别为标准 USB Audio Class 2.0 输入设备。

⚠️ 刷 boot 有变砖和丢数据风险。只给 `lavender` 用，不要刷到其他设备。

## 它会做什么

1.  手机开机后配置 USB Gadget：

    ```plaintext
    adb + uac2.0
    48000 Hz / 16-bit / mono
    ```

2.  电脑会看到一个 USB 麦克风输入设备，macOS 上通常显示为：

    ```plaintext
    Capture Inactive / Xiaomi / USB / 1 channel
    ```

3.  手机端后台运行：

    ```plaintext
    /data/adb/uac2/aaudio_uac2_bridge_rust
    ```

    桥接进程不会开机后一直占用麦克风。

    KernelSU 服务会常驻监控 USB 状态：当 USB 已连接、gadget 处于 configured 状态，并且 `dumpsys adb` 显示 `connected_to_adb=true` 时才启动桥接；拔掉 USB、ADB 授权连接消失或 USB 状态消失后会停止桥接。

4.  音频处理链：

    ```plaintext
    手机实体麦克风 -> AAudio -> 高通 -> RNNoise -> 有效声压检测 -> soft limiter -> UAC2
    ```

## 快速使用教程

### 1. 准备工具

电脑需要安装 Android platform-tools，确认命令可用：

```sh
adb version
fastboot --version
```

手机打开 USB 调试，连接电脑后确认：

```sh
adb devices
```

能看到 `device` 状态才继续。

确认设备代号必须是 `lavender`：

```sh
adb shell getprop ro.product.device
```

如果输出不是 `lavender`，不要继续刷。

### 2. 下载 Release 文件

从 GitHub Release 下载这些文件：

```plaintext
lavender-st7-defconfig-uac2-ksu-boot.img
aaudio_uac2_bridge_rust
lavender-uac2-mic-kit-services.zip
```

boot 镜像用于刷入手机。

Rust 二进制和服务脚本用于安装到 `/data/adb/`。

解压服务包：

```sh
unzip lavender-uac2-mic-kit-services.zip
cd lavender-uac2-mic-kit-services
```

### 3. 刷入 boot

重启到 fastboot：

```sh
adb reboot bootloader
```

确认 fastboot 连接：

```sh
fastboot devices
```

刷入 boot：

```sh
fastboot flash boot lavender-st7-defconfig-uac2-ksu-boot.img
fastboot reboot
```

手机启动完成后，再确认 ADB：

```sh
adb devices
```

确认 root 可用：

```sh
adb shell 'su -c id'
```

如果看到 `uid=0(root)`，继续下一步。

### 4. 安装 USB 麦克风服务

在解压后的服务包目录运行：

```sh
chmod +x install_services.sh
./install_services.sh
```

它会自动完成这些事情：

1.  检查当前设备是不是 `lavender`。
2.  检查 `su` root 是否可用。
3.  安装 Rust 桥接二进制到 `/data/adb/uac2/`。
4.  安装 KernelSU 自启动脚本到 `/data/adb/service.d/`。
5.  清理旧桥接二进制。
6.  重启手机。

不想用脚本时，可以手动执行同等命令：

```sh
adb push aaudio_uac2_bridge_rust /data/local/tmp/aaudio_uac2_bridge_rust
adb push ksu_service_uac2_adb.sh /data/local/tmp/99-uac2-adb.sh
adb push ksu_service_start_uac2_bridge.sh /data/local/tmp/100-start-uac2-bridge.sh
adb shell 'su -c "mkdir -p /data/adb/uac2 /data/adb/service.d"'
adb shell 'su -c "cp /data/local/tmp/aaudio_uac2_bridge_rust /data/adb/uac2/aaudio_uac2_bridge_rust"'
adb shell 'su -c "cp /data/local/tmp/99-uac2-adb.sh /data/adb/service.d/99-uac2-adb.sh"'
adb shell 'su -c "cp /data/local/tmp/100-start-uac2-bridge.sh /data/adb/service.d/100-start-uac2-bridge.sh"'
adb shell 'su -c "chmod 755 /data/adb/uac2/aaudio_uac2_bridge_rust /data/adb/service.d/99-uac2-adb.sh /data/adb/service.d/100-start-uac2-bridge.sh"'
adb reboot
```

### 5. 在电脑上选择麦克风

手机重启后，用 USB 连接电脑。

在系统声音设置或录音软件里选择类似下面的输入设备：

```plaintext
Capture Inactive
Xiaomi USB
USB Audio
```

此时对着手机说话，电脑端应当能看到输入电平。

## 常用检查命令

查看服务是否运行：

```sh
adb shell 'su -c "ps -A | grep aaudio_uac2_bridge_rust || true"'
```

查看 USB 触发条件：

```sh
adb shell 'su -c "cat /sys/class/power_supply/usb/online; cat /sys/class/android_usb/android0/state; dumpsys adb | grep connected_to_adb"'
```

正常连接到已授权电脑时，通常会看到：

```plaintext
1
CONFIGURED
connected_to_adb=true
```

完整检查命令：

```sh
adb shell 'su -c "dumpsys adb | grep connected_to_adb"'
```

查看 USB Gadget 和桥接日志：

```sh
adb shell 'su -c "cat /data/local/tmp/ksu-uac2-adb.log; cat /data/local/tmp/ksu-uac2-bridge.log; cat /data/local/tmp/aaudio_uac2_bridge.log"'
```

查看 UAC2 声卡是否出现：

```sh
adb shell 'su -c "cat /proc/asound/cards; cat /proc/asound/pcm"'
```

## 运行一段时间后没有输入

这个项目没有 hook Android framework，也没有 hook 系统音频服务。

桥接程序只是一个 root 后台进程：

```plaintext
AAudio 读取手机麦克风 -> Rust/RNNoise 处理 -> tinyalsa 写 UAC2 PCM
```

如果电脑睡眠、录音软件释放设备、USB 短暂重枚举、线缆抖动，UAC2 PCM 写入可能返回：

```plaintext
pcm_write failed: cannot write stream data: I/O error
```

新版本会在这个错误出现时自动关闭并重新打开 UAC2 playback。

KernelSU 服务脚本也带 watchdog，即使桥接进程异常退出，也会在 2 秒后自动重启。

从 `v0.1.2` 开始，watchdog 只在 USB/ADB 授权连接条件满足时启动桥接。

拔掉 USB 后，monitor 会停止 `aaudio_uac2_bridge_rust`，释放手机麦克风。

## 调整降噪和输入电平

默认启动参数：

```plaintext
8 0.35 0.12 0.25 180
```

含义：

```plaintext
8    软件增益
0.35 RNNoise VAD 阈值
0.12 安静时残留底噪衰减比例
0.25 原始麦克风信号混合比例，用于保留音乐和非人声
180  有效声压 RMS 阈值，超过后不再按静音处理
```

临时提高输入音量：

```sh
adb shell 'su -c "killall aaudio_uac2_bridge_rust 2>/dev/null; nohup /data/adb/uac2/aaudio_uac2_bridge_rust 10 0.35 0.12 0.25 180 >/data/local/tmp/aaudio_uac2_bridge.log 2>&1 &"'
```

临时保留更多音乐和环境声：

```sh
adb shell 'su -c "killall aaudio_uac2_bridge_rust 2>/dev/null; nohup /data/adb/uac2/aaudio_uac2_bridge_rust 8 0.35 0.12 0.45 180 >/data/local/tmp/aaudio_uac2_bridge.log 2>&1 &"'
```

永久调参可以改 `ksu_service_start_uac2_bridge.sh` 里的默认值，再重新推送到：

```plaintext
/data/adb/service.d/100-start-uac2-bridge.sh
```

## 卸载

删除自启动服务和桥接二进制：

```sh
adb shell 'su -c "rm -f /data/adb/service.d/99-uac2-adb.sh /data/adb/service.d/100-start-uac2-bridge.sh /data/adb/uac2/aaudio_uac2_bridge_rust"'
adb reboot
```

恢复原 boot 需要你自己提前备份的 boot 镜像，例如：

```sh
adb reboot bootloader
fastboot flash boot your-original-boot-backup.img
fastboot reboot
```

## 从源码构建 Rust 二进制

准备 Android NDK，并安装 Rust Android target：

```sh
rustup target add aarch64-linux-android
```

构建：

```sh
cd rust-uac2-bridge
export NDK="$ANDROID_HOME/ndk/28.2.13676358"
export NDK_BIN="$NDK/toolchains/llvm/prebuilt/darwin-x86_64/bin"
export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$NDK_BIN/aarch64-linux-android29-clang"
cargo build --release --target aarch64-linux-android
```

输出文件：

```plaintext
rust-uac2-bridge/target/aarch64-linux-android/release/uac2_bridge_rust
```

发布包里把它重命名为：

```plaintext
aaudio_uac2_bridge_rust
```

## 项目文件

```plaintext
rust-uac2-bridge/
```

Rust/RNNoise 桥接源码。

```plaintext
ksu_service_uac2_adb.sh
```

配置 `adb + uac2.0` USB Gadget 的 KernelSU 自启动脚本。

```plaintext
ksu_service_start_uac2_bridge.sh
```

启动 Rust 麦克风桥接的 KernelSU 自启动脚本。

```plaintext
install_services.sh
```

安装 Rust 二进制和 KernelSU 服务脚本的新手辅助脚本。

```plaintext
usb_uac2_adb_gadget.sh
```

手动调试 USB Gadget 时使用的脚本。

## 已验证状态

```plaintext
设备: lavender
内核: 4.19.325-st7-San-Kernel-RadiethSelunaris-R1.1.105-EOL
Root: KernelSU / su 可用
UAC2: /proc/asound/cards 出现 UAC2_Gadget
桥接: aaudio_uac2_bridge_rust 自动运行
macOS 录音: 非零输入电平，测试峰值约 -10 dBFS
```

## License

本仓库以 CC0 1.0 Universal 发布，见 `LICENSE`。

依赖项目各自的许可证仍然归原项目所有。
