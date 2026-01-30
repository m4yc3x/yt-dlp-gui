# YouTube MP3/MP4 Downloader

A GUI application built in Rust using egui for downloading YouTube videos as MP4 or extracting audio as MP3.

## Features

- **Automatic yt-dlp Updates**: Automatically downloads and updates yt-dlp to the latest version - no manual installation required!
- **Enhanced GUI Interface**: Large, easy-to-use graphical interface with improved scaling
- **Simple Format Selection**: Choose between MP4 (video) or MP3 (audio only)
- **URL Validation**: Validates YouTube URLs before processing
- **Detailed Video Information**: Displays video title, duration, uploader, and formatted view count
- **Progress Tracking**: Real-time download progress with speed and ETA
- **Custom Output Path**: Choose where to save downloaded files with folder browser
- **File Location Opening**: "Open File Location" button automatically highlights the downloaded file in Windows Explorer
- **Console Output**: Live yt-dlp console output for troubleshooting
- **Organized Layout**: Clean, organized interface with grouped sections

## Prerequisites

- **No manual setup required!** The application automatically downloads yt-dlp on first use
- **Internet connection** required for initial setup and updates

## Installation

1. Download the latest release from the releases page
2. Extract the executable to your desired location
3. Run the application - it will automatically download yt-dlp on first use!

## Usage

1. **Launch the Application**: Double-click `ytmp3.exe`
2. **Enter YouTube URL**: Paste a YouTube video URL in the input field
3. **Choose Output Folder**: Click "Browse" to select where to save the file (defaults to Downloads folder)
4. **Select Format**: Choose between:
   - 🎥 **MP4 (Video)**: Downloads highest quality video with audio
   - 🎵 **MP3 (Audio Only)**: Extracts and converts audio to MP3 (highest quality available)
5. **Fetch Video Info**: Click "🔍 Fetch Info" to retrieve video details
   - The app will check for yt-dlp updates automatically
   - View video title, duration, uploader, and view count
6. **Download**: Click the download button (MP4 or MP3) to start
7. **Monitor Progress**:
   - Watch the progress bar and status messages
   - View live yt-dlp console output
8. **Open File**: Click "📁 Open File Location" to view your downloaded file in Windows Explorer

## Supported URLs

- YouTube videos: `https://www.youtube.com/watch?v=...`
- YouTube short URLs: `https://youtu.be/...`
- Various YouTube URL formats

## Technical Details

### yt-dlp Storage
The application stores yt-dlp in a `codecs.bin` folder next to the executable. This folder is created automatically on first run and contains:
- `yt-dlp.exe`: The latest version of yt-dlp

The app checks for updates each time you fetch video info and automatically downloads newer versions when available.

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