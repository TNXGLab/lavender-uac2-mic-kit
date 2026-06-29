#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"

bridge_binary="${1:-aaudio_uac2_bridge_rust}"

if ! command -v adb >/dev/null 2>&1; then
  echo "adb not found. Install Android platform-tools first." >&2
  exit 1
fi

if [ ! -f "$bridge_binary" ]; then
  echo "missing bridge binary: $bridge_binary" >&2
  echo "Put aaudio_uac2_bridge_rust next to this script, or pass its path as the first argument." >&2
  exit 1
fi

for script in ksu_service_uac2_adb.sh ksu_service_start_uac2_bridge.sh; do
  if [ ! -f "$script" ]; then
    echo "missing service script: $script" >&2
    exit 1
  fi
done

device_state="$(adb get-state 2>/dev/null || true)"
if [ "$device_state" != "device" ]; then
  echo "adb device is not ready. Check USB debugging and run: adb devices" >&2
  exit 1
fi

codename="$(adb shell getprop ro.product.device 2>/dev/null | tr -d '\r')"
if [ "$codename" != "lavender" ]; then
  echo "this package is only for lavender, current device is: ${codename:-unknown}" >&2
  exit 1
fi

if ! adb shell 'su -c id' 2>/dev/null | grep -q 'uid=0'; then
  echo "root is not available through su. Flash the KernelSU boot first, boot Android, then retry." >&2
  exit 1
fi

adb push "$bridge_binary" /data/local/tmp/aaudio_uac2_bridge_rust
adb push ksu_service_uac2_adb.sh /data/local/tmp/99-uac2-adb.sh
adb push ksu_service_start_uac2_bridge.sh /data/local/tmp/100-start-uac2-bridge.sh

adb shell 'su -c "mkdir -p /data/adb/uac2 /data/adb/service.d"'
adb shell 'su -c "cp /data/local/tmp/aaudio_uac2_bridge_rust /data/adb/uac2/aaudio_uac2_bridge_rust"'
adb shell 'su -c "cp /data/local/tmp/99-uac2-adb.sh /data/adb/service.d/99-uac2-adb.sh"'
adb shell 'su -c "cp /data/local/tmp/100-start-uac2-bridge.sh /data/adb/service.d/100-start-uac2-bridge.sh"'
adb shell 'su -c "chmod 755 /data/adb/uac2/aaudio_uac2_bridge_rust /data/adb/service.d/99-uac2-adb.sh /data/adb/service.d/100-start-uac2-bridge.sh"'
adb shell 'su -c "rm -f /data/adb/uac2/aaudio_uac2_bridge"'

echo "installed. Rebooting device..."
adb reboot
