# FileManagerDaz

<p align="center">
  <img src="src-tauri/icons/128x128.png" alt="FileManagerDaz Logo" width="128" height="128">
</p>

<p align="center">
  <strong>A lightweight desktop utility for managing DAZ Studio content bundles.</strong>
</p>

<p align="center">
  <a href="https://github.com/yaniswav/FileManagerDaz/actions/workflows/ci.yml"><img src="https://github.com/yaniswav/FileManagerDaz/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="https://github.com/yaniswav/FileManagerDaz/releases"><img src="https://img.shields.io/github/v/release/yaniswav/FileManagerDaz" alt="Release"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="License: MIT"></a>
</p>

<p align="center">
  <a href="#features">Features</a> •
  <a href="#status">Status</a> •
  <a href="#screenshots">Screenshots</a> •
  <a href="#installation">Installation</a> •
  <a href="#building">Building</a> •
  <a href="#project-structure">Structure</a> •
  <a href="#roadmap">Roadmap</a> •
  <a href="#license">License</a>
</p>

---

FileManagerDaz handles recursive extraction of ZIP/7z/RAR archives, detects DAZ content structure, and installs files into configured DAZ libraries with safety checks and content analysis. Built with Tauri, Rust, and Svelte for a fast, native experience.

## Status

✅ **Stable** — v1.2.0. Core features are functional and actively maintained.

## Features

- **Drag & Drop Import**  Drop archives directly onto the app for instant processing
- **Recursive Extraction**  Automatically handles nested archives (ZIP within RAR within 7z, etc.)
- **Multi-Format Support**  ZIP, 7z, and RAR archives (RAR requires external `unrar`)
- **DAZ Content Detection**  Identifies content types (characters, clothing, props, poses, etc.)
- **Smart Installation**  Proposes optimal library locations based on content analysis
- **Multiple Libraries**  Configure and manage multiple DAZ content libraries
- **Task History**  Track import progress with detailed logs and retry failed imports
- **Folder Normalization**  Batch-process messy download folders into organized libraries
- **Maintenance Tools**  Detect duplicates, orphaned files, and clean up empty folders
- **Smart Uninstaller**  Safely remove products from disk and database with dry-run preview
- **Integrity Checker**  Verify installed products have all expected files on disk
- **Scene Analyzer**  Parse `.duf` scenes to identify required products and missing assets
- **Desktop Selection**  Windows Explorer-style Ctrl/Shift/Click multi-selection
- **Collections**  Organize products into custom collections with batch tagging

## Screenshots

> Screenshots coming soon - the application is currently in development preview.

## Installation

### For Users

Download the latest installer from the [Releases](https://github.com/yaniswav/FileManagerDaz/releases) page.

### For Developers

#### Prerequisites

- [Rust](https://rustup.rs/) (stable toolchain)
- [Node.js](https://nodejs.org/) (v18 or later)
- [Tauri CLI](https://tauri.app/v1/guides/getting-started/prerequisites)

#### Optional (for RAR support)

- `unrar` or `WinRAR` installed and available in PATH

#### Setup

```bash
# Clone the repository
git clone https://github.com/yaniswav/FileManagerDaz.git
cd FileManagerDaz

# Install frontend dependencies
npm install

# Run in development mode
npm run tauri dev
```

## Building

### Local Release Build

```bash
# Build optimized release
npm run tauri build
```

The installer will be generated in `src-tauri/target/release/bundle/`.

### CI/CD

This project uses GitHub Actions for automated builds. Push a tag starting with `v` (e.g., `v0.1.0`) to trigger a release build that automatically attaches the Windows installer.

## Project Structure

```
FileManagerDaz/
 src/                    # Frontend (Svelte + TypeScript)
    lib/
       api/            # Tauri command bindings
       components/     # Svelte components
       stores/         # State management
       i18n/           # Internationalization
    routes/             # SvelteKit pages
 src-tauri/              # Backend (Rust)
    src/
       core/           # Business logic (extractor, analyzer)
       commands/       # Tauri command handlers
       db/             # SQLite database operations
       config/         # Settings management
    icons/              # Application icons
 docs/                   # Documentation
    ARCHITECTURE.md     # Technical architecture
    CONFIGURATION.md    # Configuration guide
 .github/                # GitHub templates and workflows
```

## Roadmap

- [x] **Smart Uninstaller**  Safe product removal with dry-run preview
- [x] **Scene Analyzer**  Parse `.duf` scenes and cross-reference dependencies
- [x] **Integrity Checker**  Verify product file completeness on disk
- [x] **Desktop Selection**  Ctrl/Shift/Click multi-selection UX
- [ ] **Content Preview**  Preview textures and thumbnails before installation
- [ ] **Cloud Backup**  Sync library metadata to cloud storage
- [ ] **Plugin System**  Extensible architecture for custom analyzers
- [ ] **macOS/Linux Support**  Cross-platform builds

## Contributing

Contributions are welcome! Please read our [Contributing Guide](CONTRIBUTING.md) and [Code of Conduct](CODE_OF_CONDUCT.md) before submitting a pull request.

## License

This project is licensed under the MIT License  see the [LICENSE](LICENSE) file for details.

---

<p align="center">
  Made with  for the DAZ 3D community
</p>
