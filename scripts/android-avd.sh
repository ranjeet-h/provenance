#!/usr/bin/env bash
# Boot an ARM64 AVD (supplement only — never a gate) and install the debug APK.
# Usage: scripts/android-avd.sh [apk-path] [avd-name]
# Requires: Android SDK platform-tools + emulator + an arm64 system image.
set -euo pipefail

APK="${1:-apps/app/src-tauri/gen/android/app/build/outputs/apk/universal/debug/app-universal-debug.apk}"
AVD_NAME="${2:-Provenance_API35}"
APP_ID="com.provenance.app"
SDK="${ANDROID_HOME:-$HOME/Library/Android/sdk}"
EMULATOR="$SDK/emulator/emulator"
ADB="$SDK/platform-tools/adb"
SDKMANAGER="$SDK/cmdline-tools/latest/bin/sdkmanager"
AVDMANAGER="$SDK/cmdline-tools/latest/bin/avdmanager"

if [[ ! -f "$APK" ]]; then
  echo "APK not found: $APK — build it first (see scripts/android-device.sh)."
  exit 1
fi

# This repo builds arm64 only; the emulator on Apple Silicon runs arm64 images.
IMG_PKG="system-images;android-35;google_apis;arm64-v8a"
if ! "$ADB" devices 2>/dev/null | grep -q emulator; then
  if ! "$AVDMANAGER" list avd 2>/dev/null | grep -q "$AVD_NAME"; then
    echo "Installing system image $IMG_PKG ..."
    yes | "$SDKMANAGER" "$IMG_PKG" "platform-tools" "emulator" >/dev/null
    echo "no" | "$AVDMANAGER" create avd -n "$AVD_NAME" -k "$IMG_PKG" -d "pixel_8" --force
  fi
  echo "Booting $AVD_NAME (cold boot, first run takes minutes) ..."
  "$EMULATOR" -avd "$AVD_NAME" -no-snapshot -no-boot-anim -memory 4096 >/tmp/provenance-avd.log 2>&1 &
  echo "Waiting for boot ..."
  "$ADB" wait-for-device
  "$ADB" shell 'while [ "$(getprop sys.boot_completed)" != "1" ]; do sleep 2; done'
  echo "Emulator booted."
else
  echo "Emulator already running."
fi

echo "Installing $APK ..."
"$ADB" install -r "$APK"
"$ADB" shell monkey -p "$APP_ID" -c android.intent.category.LAUNCHER 1
echo "Done. Reminder: emulator results are supplements, never gate evidence."
