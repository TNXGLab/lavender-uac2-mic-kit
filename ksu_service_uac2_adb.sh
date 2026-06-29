#!/system/bin/sh

LOG=/data/local/tmp/ksu-uac2-adb.log
GADGET=/config/usb_gadget/g1
CONFIG="$GADGET/configs/b.1"
FUNCTIONS="$GADGET/functions"

exec >>"$LOG" 2>&1

echo "service start: $(date)"

for _ in $(seq 1 60); do
  [ -d "$FUNCTIONS/uac2.0" ] && [ -d "$FUNCTIONS/ffs.adb" ] && [ -d "$CONFIG" ] && break
  sleep 1
done

if [ ! -d "$FUNCTIONS/uac2.0" ]; then
  echo "uac2.0 is not available on this kernel, skip"
  exit 0
fi

UDC_NAME="$(cat "$GADGET/UDC" 2>/dev/null || true)"
if [ -z "$UDC_NAME" ]; then
  UDC_NAME="$(ls /sys/class/udc | head -n 1)"
fi

echo "unbind $UDC_NAME"
echo "" > "$GADGET/UDC" || true
sleep 2

for link in "$CONFIG"/*; do
  [ -L "$link" ] && rm -f "$link"
done

# p_* is USB-IN in this kernel's f_uac2 implementation, so macOS exposes it
# as an input/microphone stream. c_* is host playback/output.
echo 0 > "$FUNCTIONS/uac2.0/c_chmask"
echo 48000 > "$FUNCTIONS/uac2.0/c_srate"
echo 2 > "$FUNCTIONS/uac2.0/c_ssize"
echo 1 > "$FUNCTIONS/uac2.0/p_chmask"
echo 48000 > "$FUNCTIONS/uac2.0/p_srate"
echo 2 > "$FUNCTIONS/uac2.0/p_ssize"

ln -s "$FUNCTIONS/ffs.adb" "$CONFIG/ffs.adb"
ln -s "$FUNCTIONS/uac2.0" "$CONFIG/uac2.0"

echo "$UDC_NAME" > "$GADGET/UDC"
echo "bind done: $(date)"
for name in c_chmask c_srate c_ssize p_chmask p_srate p_ssize; do
  printf "%s=" "$name"
  cat "$FUNCTIONS/uac2.0/$name"
done
ls -l "$CONFIG"
