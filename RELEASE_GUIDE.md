# Release Guide

Guide for creating releases of FileManagerDaz.

## Quick Release

```bash
# 1. Ensure all changes are committed
git add .
git commit -m "chore: prepare release v1.0.0"

# 2. Create and push a tag
git tag -a v1.0.0 -m "Release v1.0.0"
git push origin v1.0.0

# 3. GitHub Actions will automatically:
#    - Build for Windows, macOS, and Linux
#    - Create a GitHub Release
#    - Upload all installers
```

## Step-by-Step Guide

### 1. Prepare Changes

Before releasing, ensure:
- All features are complete and tested
- Version numbers are updated in `package.json` and `src-tauri/tauri.conf.json`
- CHANGELOG.md is updated (if you have one)

```bash
# Update version (both files should match)
# package.json: "version": "1.0.0"
# src-tauri/tauri.conf.json: "version": "1.0.0"

# Commit version bump
git add package.json src-tauri/tauri.conf.json
git commit -m "chore: bump version to 1.0.0"
git push origin main
```

### 2. Create a Tag

Tags trigger the release workflow:

```bash
# Create annotated tag
git tag -a v1.0.0 -m "Release v1.0.0"

# Push tag to GitHub
git push origin v1.0.0
```

### 3. Monitor Build

1. Go to **Actions** tab on GitHub
2. Watch the "Release" workflow
3. Build takes approximately 15-30 minutes for all platforms

### 4. Verify Release

1. Go to **Releases** page: `https://github.com/yaniswav/FileManagerDaz/releases`
2. Verify all installers are attached:
   - Windows: `.msi` and `.exe` (NSIS)
   - macOS: `.dmg`
   - Linux: `.AppImage` and `.deb`

## Generated Files

| Platform | File | Description |
|----------|------|-------------|
| Windows | `FileManagerDaz_1.0.0_x64-setup.exe` | NSIS installer |
| Windows | `FileManagerDaz_1.0.0_x64_en-US.msi` | MSI installer |
| macOS | `FileManagerDaz_1.0.0_x64.dmg` | Disk image |
| macOS | `FileManagerDaz.app` | Application bundle (inside DMG) |
| Linux | `FileManagerDaz_1.0.0_amd64.AppImage` | Portable app |
| Linux | `FileManagerDaz_1.0.0_amd64.deb` | Debian package |

## Versioning

Follow [Semantic Versioning](https://semver.org/):

- **MAJOR** (1.x.x): Breaking changes
- **MINOR** (x.1.x): New features, backward compatible
- **PATCH** (x.x.1): Bug fixes, backward compatible

Examples:
- `v1.0.0` - Initial release
- `v1.1.0` - New feature added
- `v1.1.1` - Bug fix
- `v2.0.0` - Breaking change

## Troubleshooting

### Build Failed

1. Check the Actions log for error messages
2. Common issues:
   - Missing dependencies in `Cargo.toml`
   - TypeScript errors in frontend
   - Icon files missing

### Release Not Created

Ensure the tag matches pattern `v*`:
```bash
# Correct
git tag -a v1.0.0 -m "Release"

# Wrong (won't trigger workflow)
git tag -a 1.0.0 -m "Release"
```

### Delete and Recreate Tag

If you need to redo a release:
```bash
# Delete local tag
git tag -d v1.0.0

# Delete remote tag
git push origin :refs/tags/v1.0.0

# Delete the release on GitHub (manual)
# Then recreate the tag
git tag -a v1.0.0 -m "Release v1.0.0"
git push origin v1.0.0
```

## Local Build (Testing)

To test the build locally before releasing:

```bash
# Windows
npm run tauri:build

# Output: src-tauri/target/release/bundle/
```
