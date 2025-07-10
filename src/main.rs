#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eframe::egui;
use serde::{Deserialize, Serialize};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::io::{BufRead, BufReader};
use anyhow::Result;
use regex::Regex;

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
                                self.state = AppState::Success(format!("✅ Download completed successfully!\nSaved to: {}", path));
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
}

impl eframe::App for YtMp3App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_messages();

        let mut state_change = None;
        let mut should_start_download = false;
        let mut should_open_folder = false;

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
                AppState::Success(message) => {
                    ui.vertical_centered(|ui| {
                        ui.add_space(20.0);
                        ui.colored_label(egui::Color32::GREEN, message);
                        ui.add_space(15.0);
                        
                        ui.horizontal(|ui| {
                            if ui.add_sized([180.0, 40.0], egui::Button::new("📁 Open Folder"))
                                .clicked() {
                                should_open_folder = true;
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
        if should_open_folder {
            self.open_download_folder();
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
            let yt_dlp_path = exe_dir.join("yt-dlp.exe");
            if yt_dlp_path.exists() {
                return yt_dlp_path;
            }
        }
    }
    
    // Fallback to just "yt-dlp" if not found next to exe
    std::path::PathBuf::from("yt-dlp")
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
            return Err(anyhow::anyhow!("yt-dlp.exe not found. Please place yt-dlp.exe in the same folder as this application."));
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
            // Use best quality MP4 or fallback to best available
            args.extend_from_slice(&["--format", "best[ext=mp4]/best"]);
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
    let progress_tx = progress_sender.clone();
    let console_tx = progress_sender.clone();
    
    let progress_thread = thread::spawn(move || {
        let reader = BufReader::new(stdout);
        
        for line in reader.lines() {
            if let Ok(line) = line {
                // Send raw line to console output
                console_tx.send(AppMessage::ConsoleOutput(line.clone())).ok();
                
                // Parse for progress updates
                if let Some((progress, status)) = parse_progress_line(&line) {
                    progress_tx.send(AppMessage::DownloadProgress(progress, status)).ok();
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

    if !output.status.success() {
        let error_msg = String::from_utf8_lossy(&output.stderr);
        if error_msg.is_empty() {
            return Err(anyhow::anyhow!("yt-dlp.exe not found. Please place yt-dlp.exe in the same folder as this application."));
        }
        return Err(anyhow::anyhow!("Download failed: {}", error_msg));
    }

    progress_sender.send(AppMessage::DownloadProgress(
        1.0,
        "Download completed!".to_string(),
    )).ok();

    // Small delay to ensure the final progress message is processed
    thread::sleep(std::time::Duration::from_millis(100));

    Ok(output_path.to_string())
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
