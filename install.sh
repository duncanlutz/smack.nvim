#!/usr/bin/env bash
set -euo pipefail

REPO="duncanlutz/smack.nvim"
BINARY="smack"
PLIST="com.smack"
PLIST_PATH="/Library/LaunchDaemons/${PLIST}.plist"

echo "Installing smack..."

# Get latest release tag
TAG=$(curl -sI "https://github.com/${REPO}/releases/latest" | grep -i ^location: | sed 's|.*/||' | tr -d '\r')
if [ -z "$TAG" ]; then
  echo "Error: could not determine latest release"
  exit 1
fi
echo "Latest release: ${TAG}"

# Download and extract binary
ARCHIVE="smack_${TAG}_darwin_arm64.tar.gz"
URL="https://github.com/${REPO}/releases/download/${TAG}/${ARCHIVE}"
TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT

echo "Downloading ${URL}..."
curl -fL "$URL" -o "${TMPDIR}/${ARCHIVE}"
tar -xzf "${TMPDIR}/${ARCHIVE}" -C "$TMPDIR"

echo "Installing binary to /usr/local/bin (requires sudo)..."
sudo mv "${TMPDIR}/${BINARY}" /usr/local/bin/
sudo chmod +x /usr/local/bin/${BINARY}

# Install LaunchDaemon
echo "Installing LaunchDaemon..."
sudo tee "$PLIST_PATH" > /dev/null <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.smack</string>
    <key>ProgramArguments</key>
    <array>
        <string>/usr/local/bin/smack</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>StandardErrorPath</key>
    <string>/tmp/smack.log</string>
</dict>
</plist>
PLIST

# Unload existing daemon if present, then load
sudo launchctl unload "$PLIST_PATH" 2>/dev/null || true
sudo launchctl load "$PLIST_PATH"

echo ""
echo "smack ${TAG} installed and running!"
echo "Logs: /tmp/smack.log"
echo ""
echo "To uninstall:"
echo "  sudo launchctl unload ${PLIST_PATH}"
echo "  sudo rm ${PLIST_PATH} /usr/local/bin/${BINARY}"
