use std::env;
use std::fs;
use std::path::Path;

fn main() {
    // Path to the default config
    let config_src = "scrcpy_device_config.default.json";
    // Only copy if building in release mode
    let profile = env::var("PROFILE").unwrap_or_default();
    if profile == "release" {
        let out_dir = Path::new("target/release");
        let config_dst = out_dir.join("scrcpy_device_config.default.json");
        // Copy the file
        if let Err(e) = fs::copy(config_src, &config_dst) {
            panic!("Failed to copy config: {}", e);
        }
    }
    // Invalidate the build if the config changes
    println!("cargo:rerun-if-changed={}", config_src);
}
