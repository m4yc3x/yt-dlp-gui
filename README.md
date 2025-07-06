# YouTube MP3/MP4 Downloader

A GUI application built in Rust using egui for downloading YouTube videos as MP4 or extracting audio as MP3.

## Features

- **Enhanced GUI Interface**: Large, easy-to-use graphical interface with improved scaling
- **URL Validation**: Validates YouTube URLs before processing
- **Detailed Video Information**: Displays video title, duration, uploader, and formatted view count
- **Advanced Format Selection**: Detailed format descriptions including:
  - Video resolution (1080p, 720p, etc.)
  - Frame rate (30fps, 60fps, etc.)
  - Video and audio bitrates
  - Codec information (H.264, VP9, AAC, etc.)
  - File sizes
  - Format types (video-only, audio-only, or combined)
- **Audio Extraction**: Option to download audio-only as MP3
- **Progress Tracking**: Real-time download progress indication
- **Custom Output Path**: Choose where to save downloaded files
- **Organized Layout**: Clean, organized interface with grouped sections and emoji icons

## Prerequisites

- **yt-dlp**: This application requires `yt-dlp` to be installed and available in your system PATH, or placed next to the executable.
  - Download from: https://github.com/yt-dlp/yt-dlp/releases
  - For Windows: Download `yt-dlp.exe` and place it in the same folder as `ytmp3.exe`

## Installation

1. Download the latest release from the releases page
2. Extract the executable to your desired location
3. Ensure `yt-dlp` is available (see prerequisites)
4. Run the application

## Usage

1. **Launch the Application**: Double-click `ytmp3.exe`
2. **Enter YouTube URL**: Paste a YouTube video URL in the input field
3. **Fetch Video Info**: Click "Fetch Info" to retrieve video details
4. **Select Format**: Choose your preferred video/audio format from the list
5. **Choose Output**: 
   - Select output folder using the "Browse" button
   - Check "Download audio only (MP3)" if you want just the audio
6. **Download**: Click "Download" to start the download process
7. **Monitor Progress**: Watch the progress bar and status messages

## Supported URLs

- YouTube videos: `https://www.youtube.com/watch?v=...`
- YouTube short URLs: `https://youtu.be/...`
- Various YouTube URL formats

## Building from Source

### Requirements

- Rust 1.70 or later
- Cargo

### Build Steps

```bash
# Clone the repository
git clone <repository-url>
cd ytmp3

# Build the application
cargo build --release

# The executable will be in target/release/ytmp3.exe
```

## Dependencies

- **eframe/egui**: GUI framework
- **tokio**: Async runtime
- **serde**: JSON serialization
- **reqwest**: HTTP client
- **anyhow**: Error handling
- **regex**: URL validation
- **rfd**: File dialogs
- **dirs**: Directory utilities

## License

This project is licensed under the MIT License.

## Disclaimer

This tool is for educational and personal use only. Please respect YouTube's Terms of Service and copyright laws. Only download content you have permission to download. 