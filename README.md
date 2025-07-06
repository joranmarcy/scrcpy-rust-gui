# scrcpy-rust GUI

This project is a Rust GUI for launching and controlling scrcpy. It uses the `eframe` (egui) framework for the interface.

## Getting Started

1. Install Rust: https://rustup.rs/
2. Install scrcpy and ensure it is in your PATH.
3. Build and run the app:
   ```sh
   cargo run
   ```

## Building for Release

To build a release binary and automatically copy the default config to the output folder:

```powershell
cargo build --release
```

This will copy `scrcpy_device_config.default.json` to `target/release/scrcpy_device_config.json` automatically via the custom build script.

## Features
- Device selection (if multiple devices are connected)
- Set resolution and bit-rate
- Launch scrcpy as a subprocess
- Loads device config from `scrcpy_device_config.json` (or falls back to `scrcpy_device_config.default.json`)
- Downloads config from a remote URL if enabled

---

This project is in early development. Contributions are welcome!
