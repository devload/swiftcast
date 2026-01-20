# SwiftCast User Guide

A comprehensive guide to using SwiftCast for AI provider switching and usage monitoring.

## Table of Contents

1. [Getting Started](#getting-started)
2. [Dashboard](#dashboard)
3. [Account Management](#account-management)
4. [Session Management](#session-management)
5. [Usage Monitoring](#usage-monitoring)
6. [Settings](#settings)
7. [Troubleshooting](#troubleshooting)

---

## Getting Started

### Installation

#### macOS
1. Download `SwiftCast_x.x.x_aarch64.dmg` from [Releases](https://github.com/devload/swiftcast/releases)
2. Open the DMG file
3. Drag SwiftCast.app to your Applications folder
4. Launch SwiftCast

#### Windows
1. Download `SwiftCast_x.x.x_x64-setup.exe` from [Releases](https://github.com/devload/swiftcast/releases)
2. Run the installer
3. Follow the installation wizard
4. Launch SwiftCast from Start Menu

### First Launch

When you first launch SwiftCast:
1. The proxy server starts automatically on port 32080
2. Claude Code's `settings.json` is configured automatically
3. You're ready to add your first account!

---

## Dashboard

![Dashboard](01-dashboard.png)

The Dashboard is your control center for the proxy server.

### Proxy Control

- **Active Account**: Shows the currently active AI provider account
- **Stop/Start Proxy**: Toggle the proxy server on/off
- **Port**: The local port number (default: 32080)
- **Status**: Shows "Running" (green) or "Stopped" (red)
- **Auto Start**: Toggle automatic proxy start on app launch

### Claude Code Settings

The yellow box shows the current Claude Code configuration:
- File path: `~/.claude/settings.json`
- Configuration preview showing `ANTHROPIC_BASE_URL`

---

## Account Management

![Account Management](02-accounts.png)

Manage your AI provider accounts here.

### Auto Scan (macOS only)

Click **Auto Scan** to automatically import Claude credentials from macOS Keychain:
- Finds existing Claude OAuth tokens
- Creates "Anthropic Official" account automatically
- No manual API key entry needed!

### Adding an Account Manually

1. Click **+ Add Account**
2. Fill in the form:
   - **Account Name**: A friendly name (e.g., "My GLM Account")
   - **Base URL**: Select the provider:
     - Anthropic (Claude): `https://api.anthropic.com`
     - GLM (Z.AI): `https://api.z.ai/api/anthropic`
   - **API Key**: Your API key for the selected provider
3. Click **Add**

### Managing Accounts

Each account card shows:
- Account name and base URL
- **Active** badge (blue) for the current account
- **Activate** button to switch to this account
- **Delete** button to remove the account

### Switching Providers

1. Click **Activate** on the desired account
2. The proxy automatically routes to the new provider
3. Your next Claude Code request will use the new provider

---

## Session Management

![Session Management](03-sessions.png)

Manage multiple Claude Code sessions with different configurations.

### What is a Session?

Each Claude Code instance creates a unique session (identified by a trace ID). SwiftCast tracks these sessions and allows you to:
- Assign different vendors to different sessions
- Override models per session
- Monitor usage per session

### Session Card

Each session displays:
- **Session ID**: First 8 characters of the trace ID
- **Time**: How long ago the session was active
- **Last Message**: The most recent user message (helps identify the session)
- **Vendor**: Dropdown to select the AI provider account
- **Model**: Dropdown to override the model (or keep original)
- **Stats**: Request count and token usage

### Changing Session Configuration

1. Find the session you want to modify
2. Select a different **Vendor** from the dropdown
3. Optionally select a **Model** override
4. Changes apply immediately to the next request

### Use Case: Different Models for Different Tasks

- Session A (complex refactoring): Anthropic + Claude Opus 4
- Session B (simple edits): Anthropic + Claude Haiku
- Session C (alternative provider): GLM + GLM-4

---

## Usage Monitoring

![Usage Monitoring](04-usage.png)

Track your API usage across all sessions and providers.

### Overview Tab

Displays aggregate statistics:
- **Requests**: Total number of API calls
- **Input Tokens**: Tokens sent to the API
- **Output Tokens**: Tokens received from the API
- **Total Tokens**: Combined usage

### By Model Tab

Breaks down usage by AI model:
- claude-sonnet-4-20250514
- claude-opus-4-20250514
- claude-3-5-haiku-20241022
- etc.

### Daily Tab

Shows usage trends over the past 7 days:
- Daily request counts
- Daily token consumption
- Helps identify usage patterns

### By Session Tab

Usage breakdown per Claude Code session:
- Useful for project-based tracking
- Compare usage across different tasks

### Recent Logs Tab

Individual request log with:
- Timestamp
- Model used
- Input/output tokens
- Session ID

---

## Settings

![Settings](05-settings.png)

Configure SwiftCast behavior.

### Language

Select your preferred language:
- English
- Korean (한국어)
- Japanese (日本語)
- Chinese (中文)

### Proxy Port

Change the local proxy port (default: 32080):
1. Click **Change**
2. Enter new port number
3. Restart Claude Code to use the new port

### Auto Start

Toggle automatic proxy start when SwiftCast launches.

### Claude Code Settings File

Shows the path to the Claude Code configuration file:
- macOS: `~/.claude/settings.json`
- Windows: `%USERPROFILE%\.claude\settings.json`

This file is automatically managed by SwiftCast.

### Data Management

**Clear Usage Logs**: Delete all usage history and statistics.

---

## Troubleshooting

### Claude Code not connecting

1. Check that the proxy is running (green "Running" status)
2. Verify the port number matches
3. Restart Claude Code after changing settings

### "No active account" error

1. Go to Account Management
2. Click **Activate** on an account
3. Or add a new account if none exist

### Auto Scan not finding credentials

Auto Scan only works on macOS and requires:
- Previous Claude Code authentication
- Credentials stored in macOS Keychain

If not found, add accounts manually with your API key.

### High token usage

1. Check Usage Monitoring > By Session
2. Identify which session is consuming tokens
3. Consider using a lighter model for simple tasks

### Session not appearing

Sessions only appear after:
- At least one request through the proxy
- Activity within the last 24 hours

---

## Tips & Best Practices

### Cost Optimization

1. Use **Session Management** to assign cheaper models to simple tasks
2. Monitor **Daily** usage to track spending trends
3. Use GLM as a fallback when Claude limits are reached

### Multi-Provider Setup

1. Register both Anthropic and GLM accounts
2. Keep one as primary, one as backup
3. Switch instantly when hitting rate limits

### Privacy

- API keys are stored locally in encrypted storage
- No data is sent to external servers (except your chosen AI provider)
- All traffic goes through your local machine

---

## Keyboard Shortcuts

| Action | macOS | Windows |
|--------|-------|---------|
| Show/Hide Window | Click tray icon | Click tray icon |
| Quit App | Tray > Quit | Tray > Quit |

---

## Getting Help

- **GitHub Issues**: [Report bugs or request features](https://github.com/devload/swiftcast/issues)
- **Releases**: [Download latest version](https://github.com/devload/swiftcast/releases)

---

*SwiftCast is open source under the MIT License.*
