# smack.nvim

Neovim plugin that detects when you physically hit your MacBook and undoes your
changes. Hit it harder, undo more.

Uses the Apple Silicon accelerometer (Bosch BMI286 IMU) to detect impacts and
classify them into three severity tiers. Includes a screen shake effect for
maximum feedback.

| Hit | g-force | Undos | Shake |
| --- | ------- | ----- | ----- |
| Light tap | > 0.3g | 1 | subtle |
| Medium hit | > 1.0g | 3 | moderate |
| Hard smack | > 2.0g | 5 | violent |

## Requirements

- Apple Silicon MacBook (M2 or later)
- macOS
- Neovim 0.9+
- Rust toolchain (to build `smack`)

> **Note:** Does not work on M1, Intel Macs, Mac Studio, Mac Mini, iMac, or Mac
> Pro — they lack the MEMS accelerometer.

## Installation

### 1. Install the `smack` binary

#### Download prebuilt binary (recommended)

Grab the latest release from
[GitHub Releases](https://github.com/duncandoit/smack.nvim/releases):

```bash
# Download and extract
curl -L https://github.com/duncandoit/smack.nvim/releases/latest/download/smack_v0.1.0_darwin_arm64.tar.gz | tar xz
sudo mv smack /usr/local/bin/
```

#### Build from source

```bash
cd smack
cargo build --release
sudo cp target/release/smack /usr/local/bin/
```

### 2. Install the plugin

#### [lazy.nvim](https://github.com/folke/lazy.nvim)

```lua
{
  dir = "~/code/smack.nvim",
  config = function()
    require("smack").setup()
  end,
}
```

#### [packer.nvim](https://github.com/wbthomason/packer.nvim)

```lua
use {
  "~/code/smack.nvim",
  config = function()
    require("smack").setup()
  end,
}
```

#### [vim-plug](https://github.com/junegunn/vim-plug)

```vim
Plug '~/code/smack.nvim'
```

```lua
-- in after/plugin/smack.lua or init.lua
require("smack").setup()
```

#### Manual

Add to your `init.lua`:

```lua
vim.opt.rtp:prepend("~/code/smack.nvim")
require("smack").setup()
```

### 3. Start the sensor daemon

`smack` requires root for IOKit HID access. You can either run it manually:

```bash
sudo smack
```

Or install it as a Launch Daemon so it starts automatically on boot:

```bash
sudo cp com.smack.plist /Library/LaunchDaemons/
sudo launchctl load /Library/LaunchDaemons/com.smack.plist
```

To uninstall the daemon:

```bash
sudo launchctl unload /Library/LaunchDaemons/com.smack.plist
sudo rm /Library/LaunchDaemons/com.smack.plist
```

## Usage

The plugin auto-connects to `smack` on startup. Then just hit your laptop.

### Commands

| Command | Description |
| --- | --- |
| `:SmackStart` | Connect to the smack daemon |
| `:SmackStop` | Disconnect |
| `:SmackToggle` | Toggle connection |

### Configuration

```lua
require("smack").setup({
  socket_path = "/tmp/smack.sock", -- smack daemon socket
  enabled = true,                  -- auto-connect on startup
  shake = true,                    -- screen shake effect
  undo_count = {                   -- undos per severity
    light = 1,
    medium = 3,
    hard = 5,
  },
  shake_intensity = {              -- shake amplitude per severity
    light = 1,
    medium = 3,
    hard = 5,
  },
})
```

## Architecture

```
┌─────────────────┐     Unix socket      ┌─────────────────┐
│  smack (Rust)   │ ──────────────────▶   │  smack.nvim     │
│                 │   /tmp/smack.sock     │  (Lua)          │
│  IOKit HID      │                       │                 │
│  ▸ read accel   │   JSON events:        │  ▸ parse event  │
│  ▸ detect hits  │   {"severity":"hard", │  ▸ undo N times │
│  ▸ broadcast    │    "amplitude":2.34,  │  ▸ screen shake │
│                 │    "undos":5}         │  ▸ notify       │
└─────────────────┘                       └─────────────────┘
     (sudo)                              (multiple instances)
```

`smack` runs as root (required for accelerometer access), broadcasts hit events
over a Unix socket. Any number of Neovim instances can connect simultaneously.

## Credits

Accelerometer interface ported from
[olvvier/apple-silicon-accelerometer](https://github.com/olvvier/apple-silicon-accelerometer).
Inspired by [spank](https://github.com/taigrr/spank).

## License

MIT
