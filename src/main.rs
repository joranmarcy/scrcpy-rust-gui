use eframe::egui;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Deserialize)]
struct DeviceConfig {
    label: String,
    scrcpy_args: String,
}

struct ScrcpyGuiApp {
    devices: Vec<String>,
    selected_device: usize,
    last_selected_device: usize,
    scrcpy_process: Option<Child>,
    device_type: String,
    crop_args: Option<String>,
    applied_config: String,
    last_refresh: Instant,
    device_config: HashMap<String, DeviceConfig>,
    config_url: String,
    auto_download_on_start: bool, // NEW: auto download config on start
    status_message: String, // NEW: for visual feedback
}

impl Default for ScrcpyGuiApp {
    fn default() -> Self {
        let config_url = "https://example.com/scrcpy_device_config.json".to_string();
        // Load auto_download_on_start from settings.json
        let settings_path = "settings.json";
        let auto_download_on_start = match std::fs::read_to_string(settings_path) {
            Ok(data) => {
                serde_json::from_str::<serde_json::Value>(&data)
                    .ok()
                    .and_then(|v| v.get("auto_download_on_start").and_then(|b| b.as_bool()))
                    .unwrap_or(true)
            },
            Err(_) => true,
        };
        let mut status_message = String::new();
        if auto_download_on_start {
            match ScrcpyGuiApp::download_and_update_device_config(&config_url, "scrcpy_device_config.json") {
                Ok(_) => status_message = "Config downloaded successfully.".to_string(),
                Err(e) => status_message = format!("Failed to download config: {}", e),
            }
        }
        let devices = Self::get_adb_devices();
        let config: HashMap<String, DeviceConfig> = {
            let main_path = "scrcpy_device_config.json";
            let default_path = "scrcpy_device_config.default.json";
            let try_load = |path: &str| -> Option<HashMap<String, DeviceConfig>> {
                match fs::read_to_string(path) {
                    Ok(data) => match serde_json::from_str(&data) {
                        Ok(cfg) => Some(cfg),
                        Err(e) => {
                            eprintln!("Failed to parse {}: {}", path, e);
                            None
                        }
                    },
                    Err(e) => {
                        eprintln!("Failed to read {}: {}", path, e);
                        None
                    }
                }
            };
            match try_load(main_path) {
                Some(cfg) => cfg,
                None => {
                    status_message = "Loaded default config (fallback).".to_string();
                    try_load(default_path).unwrap_or_else(HashMap::new)
                }
            }
        };
        let mut app = Self {
            devices: devices.clone(),
            selected_device: 0,
            last_selected_device: usize::MAX,
            scrcpy_process: None,
            device_type: String::new(),
            crop_args: None,
            applied_config: String::new(),
            last_refresh: Instant::now(),
            device_config: config,
            config_url,
            auto_download_on_start,
            status_message,
        };
        app.detect_and_apply_device_type();
        app
    }
}

impl ScrcpyGuiApp {
    fn get_adb_devices() -> Vec<String> {
        let output = Command::new("adb")
            .arg("devices")
            .stdout(Stdio::piped())
            .output();
        if let Ok(output) = output {
            let text = String::from_utf8_lossy(&output.stdout);
            text.lines()
                .skip(1)
                .filter_map(|line| {
                    let parts: Vec<_> = line.split_whitespace().collect();
                    if parts.len() == 2 && parts[1] == "device" {
                        Some(parts[0].to_string())
                    } else {
                        None
                    }
                })
                .collect()
        } else {
            vec![]
        }
    }

    fn get_scrcpy_version() -> String {
        let output = Command::new("scrcpy")
            .arg("--version")
            .stdout(Stdio::piped())
            .output();
        if let Ok(output) = output {
            let text = String::from_utf8_lossy(&output.stdout);
            // Extract version number (e.g., 'scrcpy 3.3.1 <...>' -> '3.3.1')
            let first_line = text.lines().next().unwrap_or("Unknown");
            let mut parts = first_line.split_whitespace();
            if let (Some(_), Some(version)) = (parts.next(), parts.next()) {
                version.to_string()
            } else {
                "Unknown".to_string()
            }
        } else {
            "scrcpy not found".to_string()
        }
    }

    fn get_device_type(serial: &str) -> String {
        let output = Command::new("adb")
            .arg("-s").arg(serial)
            .arg("shell")
            .arg("getprop ro.product.model")
            .stdout(Stdio::piped())
            .output();
        if let Ok(output) = output {
            let text = String::from_utf8_lossy(&output.stdout);
            text.trim().to_string()
        } else {
            "Unknown".to_string()
        }
    }

    fn detect_and_apply_device_type(&mut self) {
        if self.devices.is_empty() { return; }
        let serial = &self.devices[self.selected_device];
        let dev_type = Self::get_device_type(serial);
        // Avoid double borrow by splitting logic
        let config = self.device_config.get(&dev_type).or_else(|| self.device_config.get("default")).cloned();
        if let Some(cfg) = config {
            self.apply_crop(&cfg.scrcpy_args);
            self.device_type = cfg.label;
        } else {
            self.apply_crop("");
        }
    }

    fn apply_crop(&mut self, crop: &str) {
        self.crop_args = None;
        self.applied_config = crop.to_string();
        let mut crop_args = Vec::new();
        let mut iter = crop.split_whitespace().peekable();
        while let Some(part) = iter.next() {
            match part {
                s if s.starts_with("--crop") => {
                    crop_args.push(s.to_string());
                },
                "-m" => {
                    if let Some(_val) = iter.next() {
                        // max_size removed from struct/UI
                    }
                },
                "-b" | "--video-bit-rate" => {
                    if let Some(_val) = iter.next() {
                        // video_bitrate removed from struct/UI
                    }
                },
                _ => {
                    crop_args.push(part.to_string());
                    if let Some(next) = iter.peek() {
                        if !next.starts_with('-') {
                            crop_args.push(iter.next().unwrap().to_string());
                        }
                    }
                }
            }
        }
        if !crop_args.is_empty() {
            self.crop_args = Some(crop_args.join(" "));
        }
    }

    fn refresh_devices(&mut self) {
        let devices = Self::get_adb_devices();
        if devices != self.devices {
            self.devices = devices;
            if self.selected_device >= self.devices.len() {
                self.selected_device = 0;
            }
            self.last_selected_device = usize::MAX;
            self.detect_and_apply_device_type();
        }
    }

    fn download_and_update_device_config(url: &str, path: &str) -> std::io::Result<()> {
        let resp = reqwest::blocking::get(url).expect("Failed to download device config");
        let text = resp.text().expect("Failed to read response text");
        std::fs::write(path, text)
    }
}

impl eframe::App for ScrcpyGuiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.request_repaint();
        if self.last_refresh.elapsed() > Duration::from_secs(1) {
            self.last_refresh = Instant::now();
            self.refresh_devices();
        }
        if !self.devices.is_empty() && self.selected_device != self.last_selected_device {
            self.last_selected_device = self.selected_device;
            self.detect_and_apply_device_type();
        }
        let scrcpy_version = Self::get_scrcpy_version();
        egui::TopBottomPanel::top("top_bar").show(ctx, |ui| {
            ui.horizontal_centered(|ui| {
                ui.heading("📱 scrcpy GUI");
                ui.label(egui::RichText::new(format!("v{}", scrcpy_version)).color(egui::Color32::LIGHT_BLUE).size(16.0));
            });
        });
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(8.0);
            egui::CollapsingHeader::new("Configuration").default_open(true).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Config URL:");
                    ui.text_edit_singleline(&mut self.config_url).on_hover_text("Remote JSON config for device types");
                    if ui.button("⬇ Download").on_hover_text("Download latest config from URL").clicked() {
                        match Self::download_and_update_device_config(&self.config_url, "scrcpy_device_config.json") {
                            Ok(_) => self.status_message = "✅ Config downloaded successfully.".to_string(),
                            Err(e) => self.status_message = format!("⚠️ Failed to download config: {}", e),
                        }
                    }
                });
                let changed = ui.checkbox(&mut self.auto_download_on_start, "Auto download config on start")
                    .on_hover_text("Download config at app startup").changed();
                if changed {
                    // Save to settings.json
                    let settings = serde_json::json!({
                        "auto_download_on_start": self.auto_download_on_start
                    });
                    let _ = std::fs::write("settings.json", serde_json::to_string_pretty(&settings).unwrap());
                }
            });
            ui.add_space(8.0);
            if !self.status_message.is_empty() {
                let is_error = self.status_message.to_lowercase().contains("fail") || self.status_message.to_lowercase().contains("error");
                let color = if is_error { egui::Color32::RED } else { egui::Color32::GREEN };
                let icon = if is_error { "⚠️" } else { "✅" };
                ui.horizontal(|ui| {
                    ui.colored_label(color, icon);
                    ui.colored_label(color, &self.status_message);
                });
                ui.add_space(4.0);
            }
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    ui.label("Device:");
                    if self.devices.is_empty() {
                        ui.label("No devices found");
                    } else {
                        egui::ComboBox::new("device_select", "Device")
                            .selected_text(self.devices[self.selected_device].clone())
                            .show_ui(ui, |ui| {
                                for (i, dev) in self.devices.iter().enumerate() {
                                    ui.selectable_value(&mut self.selected_device, i, dev);
                                }
                            });
                        if ui.button("↻").on_hover_text("Refresh device list").clicked() {
                            self.refresh_devices();
                        }
                    }
                });
            });
            ui.add_space(8.0);
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(format!("Device type: ")).strong());
                    ui.label(egui::RichText::new(&self.device_type).color(egui::Color32::YELLOW));
                });
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Applied config:").strong());
                    ui.label(egui::RichText::new(&self.applied_config).color(egui::Color32::LIGHT_GREEN));
                });
            });
            ui.add_space(8.0);
            egui::CollapsingHeader::new("Advanced").default_open(false).show(ui, |ui| {
                if self.device_config.is_empty() {
                    ui.colored_label(egui::Color32::RED, "Device config missing or invalid!");
                } else {
                    ui.label("Loaded device configs:");
                    egui::ScrollArea::vertical().max_height(100.0).show(ui, |ui| {
                        for (k, v) in &self.device_config {
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new(k).strong());
                                ui.label(format!("{} | {}", v.label, v.scrcpy_args));
                            });
                        }
                    });
                }
            });
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui.add_enabled(!self.devices.is_empty() && self.scrcpy_process.is_none(), egui::Button::new("▶ Start scrcpy")).on_hover_text("Launch scrcpy for selected device").clicked() {
                    let mut cmd = Command::new("scrcpy");
                    if !self.devices.is_empty() {
                        cmd.arg("--serial").arg(&self.devices[self.selected_device]);
                    }
                    if let Some(ref crop) = self.crop_args {
                        for arg in crop.split_whitespace() {
                            cmd.arg(arg);
                        }
                    }
                    match cmd.spawn() {
                        Ok(child) => self.scrcpy_process = Some(child),
                        Err(e) => self.status_message = format!("⚠️ Failed to start scrcpy: {}", e),
                    }
                }
                if ui.add_enabled(self.scrcpy_process.is_some(), egui::Button::new("⏹ Stop scrcpy")).on_hover_text("Stop running scrcpy process").clicked() {
                    if let Some(child) = &mut self.scrcpy_process {
                        let _ = child.kill();
                    }
                    self.scrcpy_process = None;
                }
            });
            ui.add_space(8.0);
        });
        egui::TopBottomPanel::bottom("footer").show(ctx, |ui| {
            ui.horizontal_centered(|ui| {
                ui.hyperlink_to("scrcpy project", "https://github.com/Genymobile/scrcpy");
                ui.label("| GUI by joran@2025");
            });
        });
    }
}

fn main() {
    let options = eframe::NativeOptions::default();
    let _ = eframe::run_native(
        "scrcpy GUI",
        options,
        Box::new(|_cc| Ok(Box::new(ScrcpyGuiApp::default()))),
    );
}
