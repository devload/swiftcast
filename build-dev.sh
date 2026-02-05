#!/bin/bash
# SwiftCast Dev Build Script
# Builds a development version with separate port (32081) and database (data-dev.db)

set -e

echo "🔧 Building SwiftCast DEV version..."

# Set dev mode for build
export SWIFTCAST_DEV=1

# Temporarily modify tauri.conf.json for dev build
CONF_FILE="src-tauri/tauri.conf.json"
cp "$CONF_FILE" "$CONF_FILE.bak"

# Update product name and identifier for dev
sed -i '' 's/"productName": "SwiftCast"/"productName": "SwiftCast-Dev"/' "$CONF_FILE"
sed -i '' 's/"identifier": "com.swiftcast.desktop"/"identifier": "com.swiftcast.desktop.dev"/' "$CONF_FILE"
sed -i '' 's/"title": "SwiftCast"/"title": "SwiftCast (Dev)"/' "$CONF_FILE"

# Build
npm run tauri build

# Restore original config
mv "$CONF_FILE.bak" "$CONF_FILE"

# Copy to Applications with -Dev suffix
echo "📦 Installing SwiftCast-Dev.app..."
rm -rf /Applications/SwiftCast-Dev.app
cp -r src-tauri/target/release/bundle/macos/SwiftCast-Dev.app /Applications/

echo "✅ SwiftCast-Dev installed!"
echo ""
echo "📍 Location: /Applications/SwiftCast-Dev.app"
echo "🔌 Port: 32081"
echo "💾 Database: ~/.config/com.swiftcast.app/data-dev.db"
echo ""
echo "To run: SWIFTCAST_DEV=1 open /Applications/SwiftCast-Dev.app"
