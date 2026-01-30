#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eframe::egui;
use serde::{Deserialize, Serialize};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::io::{BufRead, BufReader};
use anyhow::Result;
use regex::Regex;
use std::sync::{Arc, Mutex};
use std::path::Path;

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct VideoInfo {
    title: String,
    duration: String,
    uploader: String,
    view_count: Option<u64>,
    thumbnail: Option<String>,
}

#[derive(Debug, Clone)]
enum AppState {
    Input,
    Loading,
    VideoInfo(VideoInfo),
    Downloading { progress: f32, status: String },
    Error(String),
    Success(String),
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum DownloadFormat {
    Mp4,
    Mp3,
}

struct YtMp3App {
    url_input: String,
    state: AppState,
    download_format: DownloadFormat,
    output_path: String,
    receiver: Option<mpsc::Receiver<AppMessage>>,
    console_output: Vec<String>,
}

#[derive(Debug)]
enum AppMessage {
    VideoInfoReceived(Result<VideoInfo>),
    DownloadProgress(f32, String),
    DownloadComplete(Result<String>),
    ConsoleOutput(String),
}

impl Default for YtMp3App {
    fn default() -> Self {
        let default_path = dirs::download_dir()
            .unwrap_or_else(|| std::env::current_dir().unwrap())
            .to_string_lossy()
            .to_string();

        Self {
            url_input: String::new(),
            state: AppState::Input,
            download_format: DownloadFormat::Mp4,
            output_path: default_path,
            receiver: None,
            console_output: Vec::new(),
        }
    }
}

impl YtMp3App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // Set larger UI scaling for better visibility
        cc.egui_ctx.set_pixels_per_point(1.25);
        
        // Configure style for better appearance
        let mut style = (*cc.egui_ctx.style()).clone();
        style.spacing.item_spacing = egui::vec2(8.0, 8.0);
        style.spacing.button_padding = egui::vec2(12.0, 8.0);
        style.spacing.indent = 25.0;
        style.text_styles.insert(
            egui::TextStyle::Body,
            egui::FontId::new(14.0, egui::FontFamily::Proportional),
        );
        style.text_styles.insert(
            egui::TextStyle::Button,
            egui::FontId::new(14.0, egui::FontFamily::Proportional),
        );
        style.text_styles.insert(
            egui::TextStyle::Heading,
            egui::FontId::new(20.0, egui::FontFamily::Proportional),
        );
        cc.egui_ctx.set_style(style);
        
        Self::default()
    }

    fn is_valid_youtube_url(&self, url: &str) -> bool {
        let youtube_regex = Regex::new(r"^(https?://)?(www\.)?(youtube\.com|youtu\.be)/.+").unwrap();
        youtube_regex.is_match(url)
    }

    fn fetch_video_info(&mut self) {
        if !self.is_valid_youtube_url(&self.url_input) {
            self.state = AppState::Error("Invalid YouTube URL".to_string());
            return;
        }

        let url = self.url_input.clone();

        // Clear previous console output
        self.console_output.clear();

        let (tx, rx) = mpsc::channel();
        self.receiver = Some(rx);
        self.state = AppState::Loading;

        thread::spawn(move || {
            // First, check and update yt-dlp
            let rt = tokio::runtime::Runtime::new().unwrap();
            if let Err(e) = rt.block_on(check_and_update_yt_dlp(&tx)) {
                tx.send(AppMessage::ConsoleOutput(format!("Update check failed: {}", e))).ok();

                // Check if yt-dlp exists at all
                let yt_dlp_path = get_yt_dlp_path();
                if !yt_dlp_path.exists() {
                    tx.send(AppMessage::VideoInfoReceived(
                        Err(anyhow::anyhow!("yt-dlp is not installed and could not be downloaded. Error: {}", e))
                    )).ok();
                    return;
                } else {
                    tx.send(AppMessage::ConsoleOutput(
                        format!("Continuing with existing yt-dlp at: {}", yt_dlp_path.display())
                    )).ok();
                }
            }

            // Then fetch video info
            let result = get_video_info(&url, &tx);
            tx.send(AppMessage::VideoInfoReceived(result)).ok();
        });
    }

    fn start_download(&mut self) {
        if let AppState::VideoInfo(_) = &self.state {
            let url = self.url_input.clone();
            let output_path = self.output_path.clone();
            let format = self.download_format;

            // Clear previous console output
            self.console_output.clear();

            let (tx, rx) = mpsc::channel();
            self.receiver = Some(rx);
            
            // Set state to downloading
            self.state = AppState::Downloading {
                progress: 0.0,
                status: "Starting download...".to_string(),
            };

            // Add debug message
            tx.send(AppMessage::ConsoleOutput("DEBUG: start_download() called, spawning thread...".to_string())).ok();

            thread::spawn(move || {
                tx.send(AppMessage::ConsoleOutput("DEBUG: Thread started, calling download_video()...".to_string())).ok();
                let result = download_video(&url, &output_path, format, &tx);
                tx.send(AppMessage::DownloadComplete(result)).ok();
            });
        } else {
            // Debug: show what state we're in
            let state_debug = match &self.state {
                AppState::Input => "Input",
                AppState::Loading => "Loading", 
                AppState::VideoInfo(_) => "VideoInfo",
                AppState::Downloading { .. } => "Downloading",
                AppState::Error(_) => "Error",
                AppState::Success(_) => "Success",
            };
            self.console_output.push(format!("DEBUG: start_download() called but state is: {}", state_debug));
        }
    }

    fn handle_messages(&mut self) {
        let mut should_clear_receiver = false;
        
        if let Some(receiver) = &self.receiver {
            while let Ok(message) = receiver.try_recv() {
                match message {
                    AppMessage::VideoInfoReceived(result) => {
                        match result {
                            Ok(video_info) => {
                                self.state = AppState::VideoInfo(video_info);
                                should_clear_receiver = true;
                            }
                            Err(e) => {
                                self.state = AppState::Error(format!("Failed to fetch video info: {}", e));
                                should_clear_receiver = true;
                            }
                        }
                    }
                    AppMessage::DownloadProgress(progress, status) => {
                        self.state = AppState::Downloading { progress, status };
                    }
                    AppMessage::DownloadComplete(result) => {
                        match result {
                            Ok(path) => {
                                self.state = AppState::Success(path);
                            }
                            Err(e) => {
                                self.state = AppState::Error(format!("❌ Download failed: {}", e));
                            }
                        }
                        should_clear_receiver = true;
                    }
                    AppMessage::ConsoleOutput(output) => {
                        self.console_output.push(output);
                        // Keep only the last 50 lines to prevent memory issues
                        if self.console_output.len() > 50 {
                            self.console_output.remove(0);
                        }
                    }
                }
            }
        }
        
        if should_clear_receiver {
            self.receiver = None;
        }
    }

    fn open_download_folder(&self) {
        // Open the download folder in the system file explorer
        #[cfg(target_os = "windows")]
        {
            std::process::Command::new("explorer")
                .arg(&self.output_path)
                .spawn()
                .ok();
        }
        
        #[cfg(target_os = "macos")]
        {
            std::process::Command::new("open")
                .arg(&self.output_path)
                .spawn()
                .ok();
        }
        
        #[cfg(target_os = "linux")]
        {
            std::process::Command::new("xdg-open")
                .arg(&self.output_path)
                .spawn()
                .ok();
        }
    }

    fn open_file_location(&self) {
        if let AppState::Success(ref file_path) = self.state {
            let path = Path::new(file_path);

            #[cfg(target_os = "windows")]
            {
                if path.is_file() {
                    // If it's a file, use /select, to highlight it in Windows Explorer
                    std::process::Command::new("explorer")
                        .arg("/select,")
                        .arg(file_path)
                        .spawn()
                        .ok();
                } else if path.is_dir() {
                    // If it's a directory, just open it
                    std::process::Command::new("explorer")
                        .arg(file_path)
                        .spawn()
                        .ok();
                } else {
                    // If path doesn't exist, try to open the parent directory and select nothing
                    if let Some(parent) = path.parent() {
                        std::process::Command::new("explorer")
                            .arg(parent.to_string_lossy().as_ref())
                            .spawn()
                            .ok();
                    }
                }
            }
            
            #[cfg(target_os = "macos")]
            {
                if path.is_file() {
                    std::process::Command::new("open")
                        .arg("-R")
                        .arg(file_path)
                        .spawn()
                        .ok();
                } else if path.is_dir() {
                    std::process::Command::new("open")
                        .arg(file_path)
                        .spawn()
                        .ok();
                } else {
                    if let Some(parent) = path.parent() {
                        std::process::Command::new("open")
                            .arg(parent.to_string_lossy().as_ref())
                            .spawn()
                            .ok();
                    }
                }
            }
            
            #[cfg(target_os = "linux")]
            {
                if path.is_file() {
                    if let Some(parent) = path.parent() {
                        std::process::Command::new("xdg-open")
                            .arg(parent.to_string_lossy().as_ref())
                            .spawn()
                            .ok();
                    }
                } else if path.is_dir() {
                    std::process::Command::new("xdg-open")
                        .arg(file_path)
                        .spawn()
                        .ok();
                } else {
                    if let Some(parent) = path.parent() {
                        std::process::Command::new("xdg-open")
                            .arg(parent.to_string_lossy().as_ref())
                            .spawn()
                            .ok();
                    }
                }
            }
        }
    }
}

impl eframe::App for YtMp3App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_messages();

        let mut state_change = None;
        let mut should_start_download = false;
        let mut should_open_location = false;

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(10.0);
            ui.heading("🎬 YouTube MP3/MP4 Downloader");
            ui.add_space(15.0);

            // URL Input Section
            ui.group(|ui| {
                ui.vertical(|ui| {
                    ui.label("📎 YouTube URL:");
                    ui.add_space(5.0);
                    ui.horizontal(|ui| {
                        ui.add(egui::TextEdit::singleline(&mut self.url_input)
                            .desired_width(500.0)
                            .hint_text("Paste YouTube URL here..."));
                        if ui.add_sized([100.0, 25.0], egui::Button::new("🔍 Fetch Info"))
                            .clicked() && !self.url_input.is_empty() {
                            self.fetch_video_info();
                        }
                    });
                });
            });

            ui.add_space(10.0);

            // Output Path Section
            ui.group(|ui| {
                ui.vertical(|ui| {
                    ui.label("📁 Output Directory:");
                    ui.add_space(5.0);
                    ui.horizontal(|ui| {
                        ui.add(egui::TextEdit::singleline(&mut self.output_path)
                            .desired_width(500.0));
                        if ui.add_sized([100.0, 25.0], egui::Button::new("📂 Browse"))
                            .clicked() {
                            if let Some(path) = rfd::FileDialog::new().pick_folder() {
                                self.output_path = path.display().to_string();
                            }
                        }
                    });
                });
            });

            ui.add_space(10.0);

            // Format Selection Section
            ui.group(|ui| {
                ui.vertical(|ui| {
                    ui.label("🎯 Download Format:");
                    ui.add_space(5.0);
                    ui.horizontal(|ui| {
                        ui.radio_value(&mut self.download_format, DownloadFormat::Mp4, "🎥 MP4 (Video)");
                        ui.add_space(20.0);
                        ui.radio_value(&mut self.download_format, DownloadFormat::Mp3, "🎵 MP3 (Audio Only)");
                    });
                });
            });

            ui.add_space(10.0);

            // Main Content Area
            match &self.state {
                AppState::Input => {
                    ui.vertical_centered(|ui| {
                        ui.add_space(20.0);
                        ui.label("Enter a YouTube URL above to get started.");
                        ui.add_space(10.0);
                        ui.label("Choose your preferred format and click 'Fetch Info' to begin.");
                    });
                }
                AppState::Loading => {
                    ui.vertical_centered(|ui| {
                        ui.add_space(20.0);
                        ui.spinner();
                        ui.add_space(10.0);
                        ui.label("Fetching video information...");
                        
                        ui.add_space(15.0);
                        
                        // Console output section
                        ui.group(|ui| {
                            ui.vertical(|ui| {
                                ui.label("📺 yt-dlp Console Output:");
                                ui.add_space(5.0);
                                
                                egui::ScrollArea::vertical()
                                    .max_height(200.0)
                                    .stick_to_bottom(true)
                                    .show(ui, |ui| {
                                        ui.with_layout(egui::Layout::top_down(egui::Align::LEFT), |ui| {
                                            for line in &self.console_output {
                                                ui.label(egui::RichText::new(line)
                                                    .font(egui::FontId::monospace(12.0))
                                                    .color(egui::Color32::LIGHT_GRAY));
                                            }
                                            
                                            if self.console_output.is_empty() {
                                                ui.label(egui::RichText::new("Waiting for yt-dlp output...")
                                                    .font(egui::FontId::monospace(12.0))
                                                    .color(egui::Color32::DARK_GRAY));
                                            }
                                        });
                                    });
                            });
                        });
                    });
                }
                AppState::VideoInfo(video_info) => {
                    // Video Information Section
                    ui.group(|ui| {
                        ui.vertical(|ui| {
                            ui.label("📺 Video Information");
                            ui.add_space(5.0);
                            
                            egui::Grid::new("video_info_grid")
                                .num_columns(2)
                                .spacing([10.0, 5.0])
                                .show(ui, |ui| {
                                    ui.label("🎬 Title:");
                                    ui.label(&video_info.title);
                                    ui.end_row();
                                    
                                    ui.label("⏱️ Duration:");
                                    ui.label(&video_info.duration);
                                    ui.end_row();
                                    
                                    ui.label("👤 Uploader:");
                                    ui.label(&video_info.uploader);
                                    ui.end_row();
                                    
                                    if let Some(views) = video_info.view_count {
                                        ui.label("👁️ Views:");
                                        ui.label(format_number_with_commas(views));
                                        ui.end_row();
                                    }
                                });
                        });
                    });

                    ui.add_space(15.0);

                    // Download Button
                    ui.vertical_centered(|ui| {
                        let format_text = match self.download_format {
                            DownloadFormat::Mp4 => "🎥 Download MP4",
                            DownloadFormat::Mp3 => "🎵 Download MP3",
                        };
                        
                        if ui.add_sized([200.0, 40.0], egui::Button::new(format_text))
                            .clicked() {
                            should_start_download = true;
                        }
                        
                        ui.add_space(10.0);
                        if ui.add_sized([120.0, 30.0], egui::Button::new("🔙 Back"))
                            .clicked() {
                            state_change = Some(AppState::Input);
                        }
                    });
                }
                AppState::Downloading { progress, status } => {
                    ui.vertical_centered(|ui| {
                        ui.add_space(20.0);
                        ui.label(status);
                        ui.add_space(10.0);
                        ui.add(egui::ProgressBar::new(*progress)
                            .desired_width(400.0)
                            .show_percentage());
                        
                        ui.add_space(15.0);
                        
                        // Console output section
                        ui.group(|ui| {
                            ui.vertical(|ui| {
                                ui.label("📺 yt-dlp Console Output:");
                                ui.add_space(5.0);
                                
                                egui::ScrollArea::vertical()
                                    .max_height(200.0)
                                    .stick_to_bottom(true)
                                    .show(ui, |ui| {
                                        ui.with_layout(egui::Layout::top_down(egui::Align::LEFT), |ui| {
                                            for line in &self.console_output {
                                                ui.label(egui::RichText::new(line)
                                                    .font(egui::FontId::monospace(12.0))
                                                    .color(egui::Color32::LIGHT_GRAY));
                                            }
                                            
                                            if self.console_output.is_empty() {
                                                ui.label(egui::RichText::new("Waiting for yt-dlp output...")
                                                    .font(egui::FontId::monospace(12.0))
                                                    .color(egui::Color32::DARK_GRAY));
                                            }
                                        });
                                    });
                            });
                        });
                    });
                }
                AppState::Error(error) => {
                    ui.vertical_centered(|ui| {
                        ui.add_space(20.0);
                        ui.colored_label(egui::Color32::RED, format!("❌ Error: {}", error));
                        ui.add_space(10.0);
                        if ui.button("🔄 Try Again").clicked() {
                            state_change = Some(AppState::Input);
                        }
                    });
                }
                AppState::Success(path) => {
                    ui.vertical_centered(|ui| {
                        ui.add_space(20.0);
                        ui.colored_label(egui::Color32::GREEN, format!("✅ Download completed successfully!\nSaved to: {}", path));
                        ui.add_space(15.0);
                        
                        ui.horizontal(|ui| {
                            if ui.add_sized([180.0, 40.0], egui::Button::new("📁 Open File Location"))
                                .clicked() {
                                should_open_location = true;
                            }
                            
                            ui.add_space(10.0);
                            
                            if ui.add_sized([180.0, 40.0], egui::Button::new("📥 Download Another"))
                                .clicked() {
                                state_change = Some(AppState::Input);
                                self.url_input.clear();
                            }
                        });
                    });
                }
            }
        });

        // Handle state changes after the UI update
        if let Some(new_state) = state_change {
            self.state = new_state;
        }
        
        // Handle download start separately
        if should_start_download {
            self.start_download();
        }
        
        // Handle folder opening separately
        if should_open_location {
            self.open_file_location();
        }

        // Request repaint to handle async updates
        ctx.request_repaint();
    }
}

impl YtMp3App {}

fn get_yt_dlp_path() -> std::path::PathBuf {
    // Get the directory where the current executable is located
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            // Check in codecs.bin folder first
            let codecs_dir = exe_dir.join("codecs.bin");
            let yt_dlp_in_codecs = codecs_dir.join("yt-dlp.exe");
            if yt_dlp_in_codecs.exists() {
                return yt_dlp_in_codecs;
            }

            // Legacy: check for yt-dlp.exe in root
            let yt_dlp_exe = exe_dir.join("yt-dlp.exe");
            if yt_dlp_exe.exists() {
                return yt_dlp_exe;
            }
        }
    }

    // Fallback to just "yt-dlp" if not found
    std::path::PathBuf::from("yt-dlp")
}

fn get_codecs_dir() -> Result<std::path::PathBuf> {
    let exe_path = std::env::current_exe()?;
    let exe_dir = exe_path.parent()
        .ok_or_else(|| anyhow::anyhow!("Could not determine executable directory"))?;
    Ok(exe_dir.join("codecs.bin"))
}

async fn get_current_yt_dlp_version() -> Option<String> {
    let yt_dlp_path = get_yt_dlp_path();

    if !yt_dlp_path.exists() {
        return None;
    }

    let mut command = Command::new(&yt_dlp_path);
    command.arg("--version");
    #[cfg(target_os = "windows")]
    command.creation_flags(0x08000000);

    match command.output() {
        Ok(output) if output.status.success() => {
            String::from_utf8(output.stdout).ok().map(|s| s.trim().to_string())
        }
        _ => None,
    }
}

#[derive(Deserialize)]
struct GitHubRelease {
    tag_name: String,
    assets: Vec<GitHubAsset>,
}

#[derive(Deserialize)]
struct GitHubAsset {
    name: String,
    browser_download_url: String,
}

async fn get_latest_yt_dlp_release() -> Result<GitHubRelease> {
    let client = reqwest::Client::builder()
        .user_agent("ytmp3-downloader")
        .build()?;

    let response = client
        .get("https://api.github.com/repos/yt-dlp/yt-dlp/releases/latest")
        .send()
        .await?;

    let release: GitHubRelease = response.json().await?;
    Ok(release)
}

async fn download_yt_dlp(url: &str, dest_path: &std::path::Path, progress_sender: &mpsc::Sender<AppMessage>) -> Result<()> {
    progress_sender.send(AppMessage::ConsoleOutput(format!("Download URL: {}", url))).ok();
    progress_sender.send(AppMessage::ConsoleOutput(format!("Destination: {}", dest_path.display()))).ok();

    let client = reqwest::Client::builder()
        .user_agent("ytmp3-downloader")
        .timeout(std::time::Duration::from_secs(300)) // 5 minute timeout
        .build()?;

    progress_sender.send(AppMessage::ConsoleOutput("Sending download request...".to_string())).ok();
    let response = client.get(url).send().await?;

    progress_sender.send(AppMessage::ConsoleOutput(format!("Response status: {}", response.status()))).ok();
    let bytes = response.bytes().await?;

    progress_sender.send(AppMessage::ConsoleOutput(format!("Downloaded {} bytes", bytes.len()))).ok();

    // Create parent directory if it doesn't exist
    if let Some(parent) = dest_path.parent() {
        progress_sender.send(AppMessage::ConsoleOutput(format!("Creating directory: {}", parent.display()))).ok();
        std::fs::create_dir_all(parent)?;
    }

    progress_sender.send(AppMessage::ConsoleOutput("Writing file...".to_string())).ok();
    std::fs::write(dest_path, bytes)?;

    // Verify the file was written successfully
    if dest_path.exists() {
        let metadata = std::fs::metadata(dest_path)?;
        progress_sender.send(AppMessage::ConsoleOutput(
            format!("File written successfully: {} bytes", metadata.len())
        )).ok();
    } else {
        return Err(anyhow::anyhow!("File was not created at {}", dest_path.display()));
    }

    Ok(())
}

async fn check_and_update_yt_dlp(progress_sender: &mpsc::Sender<AppMessage>) -> Result<()> {
    progress_sender.send(AppMessage::ConsoleOutput("Checking for yt-dlp updates...".to_string())).ok();

    // Get current version
    let current_version = get_current_yt_dlp_version().await;
    progress_sender.send(AppMessage::ConsoleOutput(
        format!("Current version: {}", current_version.as_deref().unwrap_or("not installed"))
    )).ok();

    // Get latest release info
    let release = match get_latest_yt_dlp_release().await {
        Ok(r) => r,
        Err(e) => {
            progress_sender.send(AppMessage::ConsoleOutput(
                format!("Could not check for updates: {}", e)
            )).ok();
            return Err(e);
        }
    };

    let latest_version = release.tag_name.clone();
    progress_sender.send(AppMessage::ConsoleOutput(
        format!("Latest version: {}", latest_version)
    )).ok();

    // Check if we need to update
    let needs_update = current_version.is_none() ||
        current_version.as_ref() != Some(&latest_version);

    if !needs_update {
        progress_sender.send(AppMessage::ConsoleOutput("yt-dlp is up to date!".to_string())).ok();
        return Ok(());
    }

    // Find the Windows executable in the assets
    let yt_dlp_asset = release.assets.iter()
        .find(|asset| asset.name == "yt-dlp.exe")
        .ok_or_else(|| anyhow::anyhow!("Could not find yt-dlp.exe in latest release"))?;

    progress_sender.send(AppMessage::ConsoleOutput(
        format!("Downloading yt-dlp {}...", latest_version)
    )).ok();

    // Download to codecs.bin folder
    let codecs_dir = get_codecs_dir()?;
    let dest_path = codecs_dir.join("yt-dlp.exe");

    download_yt_dlp(&yt_dlp_asset.browser_download_url, &dest_path, progress_sender).await?;

    progress_sender.send(AppMessage::ConsoleOutput(
        format!("Successfully downloaded yt-dlp {} to {}", latest_version, dest_path.display())
    )).ok();

    // Verify yt-dlp works by checking version
    progress_sender.send(AppMessage::ConsoleOutput("Verifying yt-dlp installation...".to_string())).ok();
    match get_current_yt_dlp_version().await {
        Some(version) => {
            progress_sender.send(AppMessage::ConsoleOutput(
                format!("Verification successful! yt-dlp version: {}", version)
            )).ok();
        }
        None => {
            progress_sender.send(AppMessage::ConsoleOutput(
                "WARNING: Could not verify yt-dlp installation".to_string()
            )).ok();
        }
    }

    Ok(())
}

fn get_video_info(url: &str, progress_sender: &mpsc::Sender<AppMessage>) -> Result<VideoInfo> {
    let yt_dlp_path = get_yt_dlp_path();
    
    progress_sender.send(AppMessage::ConsoleOutput(format!("Running: {} --dump-json --no-playlist {}", yt_dlp_path.display(), url))).ok();
    
    let mut command = Command::new(&yt_dlp_path);
    command.args(&["--dump-json", "--no-playlist", url]);
    #[cfg(target_os = "windows")]
    command.creation_flags(0x08000000);
    let output = command.output()?;

    if !output.status.success() {
        let error_msg = String::from_utf8_lossy(&output.stderr);
        progress_sender.send(AppMessage::ConsoleOutput(format!("ERROR: {}", error_msg))).ok();
        if error_msg.is_empty() {
            return Err(anyhow::anyhow!("yt-dlp not found. Please place yt-dlp.exe or yt-dlp.bin in the same folder as this application."));
        }
        return Err(anyhow::anyhow!("yt-dlp failed: {}", error_msg));
    }

    let json_str = String::from_utf8(output.stdout)?;
    progress_sender.send(AppMessage::ConsoleOutput("Successfully fetched video information".to_string())).ok();
    let json_value: serde_json::Value = serde_json::from_str(&json_str)?;

    let title = json_value["title"].as_str().unwrap_or("Unknown").to_string();
    let duration = format_duration(json_value["duration"].as_f64().unwrap_or(0.0));
    let uploader = json_value["uploader"].as_str().unwrap_or("Unknown").to_string();
    let view_count = json_value["view_count"].as_u64();
    let thumbnail = json_value["thumbnail"].as_str().map(|s| s.to_string());

    Ok(VideoInfo {
        title,
        duration,
        uploader,
        view_count,
        thumbnail,
    })
}

fn format_duration(seconds: f64) -> String {
    let total_seconds = seconds as u64;
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;

    if hours > 0 {
        format!("{}:{:02}:{:02}", hours, minutes, seconds)
    } else {
        format!("{}:{:02}", minutes, seconds)
    }
}

fn format_number_with_commas(num: u64) -> String {
    let num_str = num.to_string();
    let mut result = String::new();
    let chars: Vec<char> = num_str.chars().collect();
    
    for (i, ch) in chars.iter().enumerate() {
        if i > 0 && (chars.len() - i) % 3 == 0 {
            result.push(',');
        }
        result.push(*ch);
    }
    
    result
}

fn download_video(
    url: &str,
    output_path: &str,
    format: DownloadFormat,
    progress_sender: &mpsc::Sender<AppMessage>,
) -> Result<String> {
    progress_sender.send(AppMessage::ConsoleOutput("DEBUG: download_video() function called".to_string())).ok();
    progress_sender.send(AppMessage::DownloadProgress(
        0.0,
        "Starting download...".to_string(),
    )).ok();

    let downloaded_file = Arc::new(Mutex::new(None::<String>));

    let output_template = format!("{}\\%(title)s.%(ext)s", output_path);
    let mut args = vec![
        "--newline",
        "--no-warnings",
        "--output", &output_template,
        url,
    ];

    // Add format-specific arguments
    match format {
        DownloadFormat::Mp3 => {
            args.extend_from_slice(&["-x", "--audio-format", "mp3"]);
        }
        DownloadFormat::Mp4 => {
            // Download best video + best audio separately and merge them
            // This allows getting higher quality than pre-merged formats
            args.extend_from_slice(&["--format", "bestvideo[ext=mp4]+bestaudio[ext=m4a]/bestvideo+bestaudio/best"]);
        }
    }

    let yt_dlp_path = get_yt_dlp_path();
    
    // Log the exact command being run
    let command_str = format!("{} {}", yt_dlp_path.display(), args.join(" "));
    progress_sender.send(AppMessage::ConsoleOutput(format!("Running: {}", command_str))).ok();
    
    let mut command = Command::new(&yt_dlp_path);
    command.args(&args);
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());
    #[cfg(target_os = "windows")]
    command.creation_flags(0x08000000);
    let mut child = command.spawn()?;

    // Read stdout in a separate thread to parse progress
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();

    let progress_thread = thread::spawn({
        let progress_tx = progress_sender.clone();
        let console_tx = progress_sender.clone();
        let df_clone = downloaded_file.clone();
        move || {
            let reader = BufReader::new(stdout);
            
            for line in reader.lines() {
                if let Ok(line) = line {
                    console_tx.send(AppMessage::ConsoleOutput(line.clone())).ok();
                    
                    // Try to parse the destination file path from various yt-dlp output patterns
                    if line.contains("Destination:") {
                        if let Some(pos) = line.find("Destination:") {
                            let path = line[pos + 12..].trim().to_string();
                            console_tx.send(AppMessage::ConsoleOutput(format!("DEBUG: Found destination: {}", path))).ok();
                            *df_clone.lock().unwrap() = Some(path);
                        }
                    } else if line.contains("[download]") && line.contains("has already been downloaded") {
                        // Handle case where file was already downloaded
                        if let Some(start) = line.find("] ") {
                            if let Some(end) = line.find(" has already been downloaded") {
                                let path = line[start + 2..end].trim().to_string();
                                console_tx.send(AppMessage::ConsoleOutput(format!("DEBUG: Found existing file: {}", path))).ok();
                                *df_clone.lock().unwrap() = Some(path);
                            }
                        }
                    } else if line.contains("[Merger]") && line.contains("Merging formats into") {
                        // Handle merged file output
                        if let Some(start) = line.find("into \"") {
                            if let Some(end) = line.rfind("\"") {
                                if end > start + 6 {
                                    let path = line[start + 6..end].to_string();
                                    console_tx.send(AppMessage::ConsoleOutput(format!("DEBUG: Found merged file: {}", path))).ok();
                                    *df_clone.lock().unwrap() = Some(path);
                                }
                            }
                        }
                    }
                    
                    if let Some((progress, status)) = parse_progress_line(&line) {
                        progress_tx.send(AppMessage::DownloadProgress(progress, status)).ok();
                    }
                }
            }
        }
    });
    
    // Read stderr in a separate thread for error messages
    let error_tx = progress_sender.clone();
    let error_thread = thread::spawn(move || {
        let reader = BufReader::new(stderr);
        
        for line in reader.lines() {
            if let Ok(line) = line {
                // Send error output to console as well
                error_tx.send(AppMessage::ConsoleOutput(format!("ERROR: {}", line))).ok();
            }
        }
    });

    // Wait for the process to complete
    let output = child.wait_with_output()?;
    
    // Wait for both threads to finish
    progress_thread.join().ok();
    error_thread.join().ok();
    
    let final_path = downloaded_file.lock().unwrap().clone().unwrap_or_else(|| {
        // If we couldn't parse the destination, log it for debugging
        progress_sender.send(AppMessage::ConsoleOutput("WARNING: Could not determine exact file path from yt-dlp output".to_string())).ok();
        output_path.to_string()
    });
    
    if !output.status.success() {
        let error_msg = String::from_utf8_lossy(&output.stderr);
        if error_msg.is_empty() {
            return Err(anyhow::anyhow!("yt-dlp not found. Please place yt-dlp.exe or yt-dlp.bin in the same folder as this application."));
        }
        return Err(anyhow::anyhow!("Download failed: {}", error_msg));
    }

    progress_sender.send(AppMessage::DownloadProgress(
        1.0,
        "Download completed!".to_string(),
    )).ok();

    // Small delay to ensure the final progress message is processed
    thread::sleep(std::time::Duration::from_millis(100));

    Ok(final_path)
}

fn parse_progress_line(line: &str) -> Option<(f32, String)> {
    // yt-dlp progress format: [download] 45.2% of 123.45MiB at 1.23MiB/s ETA 00:30
    if line.contains("[download]") && line.contains("%") {
        // Extract percentage
        if let Some(percent_start) = line.find("] ") {
            let percent_part = &line[percent_start + 2..];
            if let Some(percent_end) = percent_part.find('%') {
                let percent_str = &percent_part[..percent_end];
                if let Ok(percent) = percent_str.parse::<f32>() {
                    let progress = percent / 100.0;
                    
                    // Extract additional info for status
                    let status = if line.contains(" at ") && line.contains(" ETA ") {
                        // Extract speed and ETA
                        let speed_start = line.find(" at ").unwrap() + 4;
                        let speed_end = line.find(" ETA ").unwrap();
                        let speed = &line[speed_start..speed_end];
                        
                        let eta_start = line.find(" ETA ").unwrap() + 5;
                        let eta = &line[eta_start..].trim();
                        
                        format!("Downloading... {:.1}% at {} (ETA: {})", percent, speed, eta)
                    } else {
                        format!("Downloading... {:.1}%", percent)
                    };
                    
                    return Some((progress, status));
                }
            }
        }
    }
    
    // Handle other status messages
    if line.contains("[download] Destination:") {
        return Some((0.0, "Preparing download...".to_string()));
    }
    
    if line.contains("[download] 100%") {
        return Some((1.0, "Download completed!".to_string()));
    }
    
    if line.contains("[ExtractAudio]") {
        return Some((0.9, "Extracting audio...".to_string()));
    }
    
    if line.contains("[ffmpeg]") {
        return Some((0.95, "Converting to MP3...".to_string()));
    }
    
    None
}

fn main() -> Result<(), eframe::Error> {
    env_logger::init();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([900.0, 800.0])
            .with_min_inner_size([800.0, 700.0])
            .with_resizable(true),
        ..Default::default()
    };

    eframe::run_native(
        "YouTube MP3/MP4 Downloader",
        options,
        Box::new(|cc| Ok(Box::new(YtMp3App::new(cc)))),
    )
}
