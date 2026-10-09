#!/bin/bash
set -euo pipefail
atlas_uid="$(id -u)"
if [ "$atlas_uid" = 501 ] || [ "$atlas_uid" -lt 502 ]; then
  echo 'Run this only while logged into the disposable Atlas Test account, never the human account.'
  exit 1
fi
atlas_home="$(dscl . -read "/Users/$(id -un)" NFSHomeDirectory | cut -d ' ' -f2-)"
atlas_dir="/tmp/atlas-live-$atlas_uid"
mkdir -p "$atlas_home/Library/LaunchAgents" "$atlas_home/Library/Logs/AtlasLiveTest" "$atlas_dir"
chmod 700 "$atlas_dir"
chmod +a 'theiskaa allow read,search' "$atlas_dir"
atlas_plist="$atlas_home/Library/LaunchAgents/dev.atlas.live-sessiond.plist"
cat > "$atlas_plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>Label</key><string>dev.atlas.live-sessiond</string>
<key>RunAtLoad</key><true/>
<key>KeepAlive</key><dict><key>SuccessfulExit</key><false/></dict>
<key>LimitLoadToSessionType</key><string>Aqua</string>
<key>StandardOutPath</key><string>$atlas_home/Library/Logs/AtlasLiveTest/helper.log</string>
<key>StandardErrorPath</key><string>$atlas_home/Library/Logs/AtlasLiveTest/helper.log</string>
</dict></plist>
PLIST
if launchctl print "gui/$atlas_uid/dev.atlas.live-sessiond" >/dev/null 2>&1; then
 echo 'Helper is already loaded.'
else
 launchctl bootstrap "gui/$atlas_uid" "$atlas_plist"
fi
for i in {1..20}; do [ -S "$atlas_dir/$atlas_uid.sock" ] && break; sleep 0.25; done
printf '{"op":"hello"}\n' | nc -U "$atlas_dir/$atlas_uid.sock"
if [ "${ATLAS_SKIP_PERMISSION_REQUEST:-0}" != 1 ]; then
  printf '{"op":"request-permissions"}\n' | nc -U "$atlas_dir/$atlas_uid.sock"
fi
echo ''
echo 'Then switch back to your main account WITHOUT logging this account out.'
echo 'No password is collected by this script. No TCC database is edited.'
