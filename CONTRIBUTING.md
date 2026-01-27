# Contributing to FileManagerDaz

First off, thank you for considering contributing to FileManagerDaz!

This document provides guidelines and instructions for contributing to this project.

## Table of Contents

- [Code of Conduct](#code-of-conduct)
- [Getting Started](#getting-started)
- [Development Setup](#development-setup)
- [Making Changes](#making-changes)
- [Code Style](#code-style)
- [Testing](#testing)
- [Submitting Changes](#submitting-changes)

## Code of Conduct

This project adheres to a [Code of Conduct](CODE_OF_CONDUCT.md). By participating, you are expected to uphold this code.

## Getting Started

1. **Fork the repository** on GitHub
2. **Clone your fork** locally:
   ```bash
   git clone https://github.com/yaniswav/FileManagerDaz.git
   cd FileManagerDaz
   ```
3. **Add the upstream remote**:
   ```bash
   git remote add upstream https://github.com/yaniswav/FileManagerDaz.git
   ```

## Development Setup

### Prerequisites

- **Rust** (stable toolchain): https://rustup.rs/
- **Node.js** (v18+): https://nodejs.org/
- **Tauri CLI**: npm install -g @tauri-apps/cli

### Installation

```bash
npm install
npm run tauri dev
```

### Useful Commands

| Command | Description |
|---------|-------------|
| npm run dev | Start Vite dev server only |
| npm run tauri dev | Start full Tauri app in dev mode |
| npm run build | Build frontend for production |
| npm run tauri build | Build complete application |
| npm run check | Run Svelte/TypeScript type checking |

## Code Style

### Rust (Backend)

```bash
cd src-tauri
cargo fmt
cargo clippy --all-targets --all-features
cargo check
```

### TypeScript/Svelte (Frontend)

```bash
npm run check
```

## Testing

### Before Submitting

```bash
npm run check
cd src-tauri
cargo check
cargo test
```

## Submitting Changes

1. Push your branch to your fork
2. Open a Pull Request against the main branch
3. Fill out the PR template
4. Respond to review feedback

### PR Requirements

- All CI checks pass
- Code follows style guidelines
- New features have documentation

Thank you for contributing!
