#!/system/bin/sh
set -eu

GADGET=/config/usb_gadget/g1
CONFIG="$GADGET/configs/b.1"
FUNCTIONS="$GADGET/functions"
LOG=/data/local/tmp/usb-uac2-adb.log

exec >"$LOG" 2>&1

echo "start: $(date)"

if [ ! -d "$FUNCTIONS/uac2.0" ]; then
  echo "missing $FUNCTIONS/uac2.0"
  exit 1
fi

if [ ! -d "$FUNCTIONS/ffs.adb" ]; then
  echo "missing $FUNCTIONS/ffs.adb"
  exit 1
fi

UDC_NAME="$(cat "$GADGET/UDC" 2>/dev/null || true)"
if [ -z "$UDC_NAME" ]; then
  UDC_NAME="$(ls /sys/class/udc | head -n 1)"
fi

# Keep Android's existing ADB FunctionFS session alive. Stopping sys.usb.config
# would let init tear down adbd, then the manually rebound gadget may not expose ADB.
echo "" > "$GADGET/UDC" || true
sleep 1

# Expose the phone to the host as a native USB Audio Class 2.0 capture device.
# ADB stays on FunctionFS so control/debug traffic remains available.
echo 0x18d1 > "$GADGET/idVendor"
echo 0x4ee7 > "$GADGET/idProduct"
echo 0x0200 > "$GADGET/bcdUSB"
echo 0x0100 > "$GADGET/bcdDevice"

echo 0x80 > "$CONFIG/bmAttributes"
echo 250 > "$CONFIG/MaxPower"

# In this Android 4.19 UAC2 function, p_* maps to USB-IN from the host's
# perspective, so macOS treats it as a microphone/input stream.
echo 0 > "$FUNCTIONS/uac2.0/c_chmask"
echo 48000 > "$FUNCTIONS/uac2.0/c_srate"
echo 2 > "$FUNCTIONS/uac2.0/c_ssize"
echo 1 > "$FUNCTIONS/uac2.0/p_chmask"
echo 48000 > "$FUNCTIONS/uac2.0/p_srate"
echo 2 > "$FUNCTIONS/uac2.0/p_ssize"

rm -f "$CONFIG/uac2.0" "$CONFIG/function1"
ln -s "$FUNCTIONS/uac2.0" "$CONFIG/uac2.0"

if ! find "$CONFIG" -maxdepth 1 -type l -lname '*ffs.adb' | grep -q .; then
  ln -s "$FUNCTIONS/ffs.adb" "$CONFIG/ffs.adb"
fi

echo "$UDC_NAME" > "$GADGET/UDC"

echo "udc=$UDC_NAME"
echo "config=$(getprop sys.usb.config)"
echo "state=$(getprop sys.usb.state)"
ls -l "$CONFIG"
echo "done: $(date)"
