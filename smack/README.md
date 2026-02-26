# smack

Apple Silicon accelerometer impact detector. Reads the Bosch BMI286 IMU via
IOKit HID and broadcasts hit events over a Unix socket.

Built for [smack.nvim](../README.md), but can be used standalone — any process
that reads JSON lines from a Unix socket can consume the events.

## Build

```bash
cargo build --release
# binary at target/release/smack
```

Zero external crate dependencies. Links against macOS system frameworks (IOKit,
CoreFoundation) via FFI.

## Usage

```bash
sudo smack
```

Root is required for IOKit HID access to the accelerometer.

On startup, `smack` creates a Unix socket at `/tmp/smack.sock` and logs to
stderr:

```
smack: accelerometer active
smack: socket at /tmp/smack.sock
smack: waiting for impacts... (ctrl+c to quit)
```

When an impact is detected:

```
smack: hit #1 [medium  amp=1.2345g  undos=3]
```

## Output format

JSON lines on stdout and to all connected socket clients:

```json
{"severity":"light","amplitude":0.4521,"undos":1}
{"severity":"medium","amplitude":1.2345,"undos":3}
{"severity":"hard","amplitude":2.8901,"undos":5}
```

| Field | Type | Description |
| --- | --- | --- |
| `severity` | `"light"` \| `"medium"` \| `"hard"` | Impact tier |
| `amplitude` | `float` | Excess acceleration in g above baseline |
| `undos` | `int` | Suggested undo count (1, 3, or 5) |

## Detection

Simple threshold-based classifier with a ~1g exponential moving average
baseline (gravity at rest):

| Tier | Threshold | Cooldown |
| --- | --- | --- |
| Light | > 0.3g excess | 500ms |
| Medium | > 1.0g excess | 500ms |
| Hard | > 2.0g excess | 500ms |

## Sensor details

- **Hardware:** Bosch BMI286 MEMS IMU
- **Interface:** IOKit HID via `AppleSPUHIDDevice` (usage page `0xFF00`, usage `3`)
- **Raw format:** 22-byte HID reports, XYZ as Q16 fixed-point int32 at offset 6
- **Sample rate:** ~800 Hz native, decimated to ~100 Hz
- **Compatibility:** Apple Silicon MacBooks M2 and later

## Running as a daemon

See [`com.smack.plist`](../com.smack.plist) for a macOS Launch Daemon
configuration that starts `smack` automatically on boot.

## Credits

Sensor interface ported from
[olvvier/apple-silicon-accelerometer](https://github.com/olvvier/apple-silicon-accelerometer)
(Python) via [taigrr/apple-silicon-accelerometer](https://github.com/taigrr/apple-silicon-accelerometer)
(Go).

## License

MIT
