#!/system/bin/sh

LOG=/data/local/tmp/ksu-uac2-bridge.log
RUST_BRIDGE=/data/adb/uac2/aaudio_uac2_bridge_rust
BRIDGE="$RUST_BRIDGE"
GAIN="${UAC2_MIC_GAIN:-8}"
VAD_THRESHOLD="${UAC2_MIC_VAD_THRESHOLD:-0.35}"
FLOOR_ATTENUATION="${UAC2_MIC_FLOOR_ATTENUATION:-0.12}"
RAW_BLEND="${UAC2_MIC_RAW_BLEND:-0.25}"
ACTIVE_RMS_THRESHOLD="${UAC2_MIC_ACTIVE_RMS_THRESHOLD:-180}"

exec >>"$LOG" 2>&1

echo "bridge service start: $(date)"

for _ in $(seq 1 90); do
  if [ -e /dev/snd/pcmC1D0p ] && [ -x "$RUST_BRIDGE" ]; then
    break
  fi
  sleep 1
done

if [ ! -e /dev/snd/pcmC1D0p ]; then
  echo "UAC2 playback device is missing, skip bridge"
  exit 0
fi

if [ ! -x "$BRIDGE" ]; then
  echo "Rust bridge binary is missing, skip bridge"
  exit 0
fi

for pid in $(pidof aaudio_uac2_bridge_rust 2>/dev/null); do
  kill -9 "$pid" 2>/dev/null || true
done

nohup "$BRIDGE" "$GAIN" "$VAD_THRESHOLD" "$FLOOR_ATTENUATION" "$RAW_BLEND" "$ACTIVE_RMS_THRESHOLD" >/data/local/tmp/aaudio_uac2_bridge.log 2>&1 &
echo "bridge launched with Rust/RNNoise gain=$GAIN vad_threshold=$VAD_THRESHOLD floor_attenuation=$FLOOR_ATTENUATION raw_blend=$RAW_BLEND active_rms_threshold=$ACTIVE_RMS_THRESHOLD"
sleep 2

cat /data/local/tmp/aaudio_uac2_bridge.log 2>/dev/null || true
