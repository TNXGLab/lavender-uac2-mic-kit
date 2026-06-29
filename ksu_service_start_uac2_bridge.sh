#!/system/bin/sh

LOG=/data/local/tmp/ksu-uac2-bridge.log
BRIDGE_LOG=/data/local/tmp/aaudio_uac2_bridge.log
MONITOR_PID_FILE=/data/local/tmp/aaudio_uac2_bridge_monitor.pid
RUST_BRIDGE=/data/adb/uac2/aaudio_uac2_bridge_rust
ADB_KEYS=/data/misc/adb/adb_keys
GAIN="${UAC2_MIC_GAIN:-8}"
VAD_THRESHOLD="${UAC2_MIC_VAD_THRESHOLD:-0.35}"
FLOOR_ATTENUATION="${UAC2_MIC_FLOOR_ATTENUATION:-0.12}"
RAW_BLEND="${UAC2_MIC_RAW_BLEND:-0.25}"
ACTIVE_RMS_THRESHOLD="${UAC2_MIC_ACTIVE_RMS_THRESHOLD:-180}"

exec >>"$LOG" 2>&1

echo "bridge monitor service start: $(date)"

if [ -f "$MONITOR_PID_FILE" ]; then
  OLD_MONITOR_PID="$(cat "$MONITOR_PID_FILE" 2>/dev/null || true)"
  if [ -n "$OLD_MONITOR_PID" ]; then
    kill "$OLD_MONITOR_PID" 2>/dev/null || true
  fi
fi

for pid in $(pidof aaudio_uac2_bridge_rust 2>/dev/null); do
  kill -9 "$pid" 2>/dev/null || true
done

(
  BRIDGE_PID=""

  usb_adb_ready() {
    [ -x "$RUST_BRIDGE" ] || return 1
    [ -e /dev/snd/pcmC1D0p ] || return 1
    [ "$(cat /sys/class/power_supply/usb/online 2>/dev/null || echo 0)" = "1" ] || return 1

    USB_STATE="$(cat /sys/class/android_usb/android0/state 2>/dev/null || true)"
    UDC_STATE="$(cat /sys/class/udc/*/state 2>/dev/null | head -n 1)"
    [ "$USB_STATE" = "CONFIGURED" ] || [ "$UDC_STATE" = "configured" ] || return 1

    ADB_STATE="$(dumpsys adb 2>/dev/null | grep -m 1 'connected_to_adb=' || true)"
    if [ -n "$ADB_STATE" ]; then
      echo "$ADB_STATE" | grep -q 'connected_to_adb=true'
    else
      [ -s "$ADB_KEYS" ]
    fi
  }

  bridge_running() {
    [ -n "$BRIDGE_PID" ] && kill -0 "$BRIDGE_PID" 2>/dev/null
  }

  start_bridge() {
    echo "bridge starting at $(date)" >>"$BRIDGE_LOG"
    "$RUST_BRIDGE" "$GAIN" "$VAD_THRESHOLD" "$FLOOR_ATTENUATION" "$RAW_BLEND" "$ACTIVE_RMS_THRESHOLD" >>"$BRIDGE_LOG" 2>&1 &
    BRIDGE_PID="$!"
    echo "bridge pid=$BRIDGE_PID"
  }

  stop_bridge() {
    if bridge_running; then
      echo "bridge stopping at $(date): USB/ADB condition is not ready"
      kill "$BRIDGE_PID" 2>/dev/null || true
      sleep 1
      kill -9 "$BRIDGE_PID" 2>/dev/null || true
    fi
    BRIDGE_PID=""
  }

  while true; do
    if usb_adb_ready; then
      if ! bridge_running; then
        start_bridge
      fi
    else
      stop_bridge
    fi
    sleep 2
  done
) &

echo "$!" > "$MONITOR_PID_FILE"
echo "bridge monitor launched pid=$(cat "$MONITOR_PID_FILE")"
