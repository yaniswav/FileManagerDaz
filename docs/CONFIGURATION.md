# FileManagerDaz Configuration Guide

This document explains where FileManagerDaz stores its data and how to configure it.

## File Locations

### Windows

All application data is stored in the user's AppData directory:

```
%APPDATA%\filemanagerdaz\FileManagerDaz\data\
├── settings.json        # Application settings
├── database.db          # SQLite database
└── logs/                # Log files (if enabled)
```

Typical path:
```
C:\Users\<username>\AppData\Roaming\filemanagerdaz\FileManagerDaz\data\
```

### Temporary Files

During extraction, temporary files are stored in:

```
%TEMP%\FileManagerDaz\
└── <extraction-uuid>/   # Temporary extraction directories
```

These are automatically cleaned up after successful imports.

## Configuration File (settings.json)

### Location

```
%APPDATA%\filemanagerdaz\FileManagerDaz\data\settings.json
```

### Structure

```json
{
  "daz_libraries": [
    {
      "path": "C:\\Users\\Public\\Documents\\My DAZ 3D Library",
      "name": "My DAZ 3D Library",
      "is_default": true
    },
    {
      "path": "D:\\DAZ Content\\Custom Library",
      "name": "Custom Library",
      "is_default": false
    }
  ],
  "temp_dir": null,
  "default_destination": null,
  "unrar_path": "C:\\Program Files\\WinRAR\\UnRAR.exe",
  "sevenzip_path": null,
  "trash_archives_after_import": false,
  "dev_log_extraction_timings": false,
  "language": "en"
}
```

### Settings Explained

| Setting | Type | Description |
|---------|------|-------------|
| `daz_libraries` | array | List of configured DAZ content libraries |
| `temp_dir` | string? | Custom temporary directory (null = system default) |
| `default_destination` | string? | Default installation path for imports |
| `unrar_path` | string? | Path to UnRAR executable (auto-detected) |
| `sevenzip_path` | string? | Path to 7z executable (optional, native support available) |
| `trash_archives_after_import` | bool | Move source archives to trash after successful import |
| `dev_log_extraction_timings` | bool | Enable detailed timing logs (developer mode) |
| `language` | string | UI language: "en" or "fr" |

## Database (database.db)

### Location

```
%APPDATA%\filemanagerdaz\FileManagerDaz\data\database.db
```

### Tables

- **products**: Installed content tracking
- **import_tasks**: Import history and task state

### Viewing the Database

You can inspect the database using any SQLite viewer:

- [DB Browser for SQLite](https://sqlitebrowser.org/)
- [SQLite CLI](https://sqlite.org/cli.html)

```bash
sqlite3 "%APPDATA%\filemanagerdaz\FileManagerDaz\data\database.db"
.tables
.schema products
SELECT * FROM products LIMIT 10;
```

## DAZ Libraries

### Auto-Detection

FileManagerDaz automatically detects common DAZ library locations:

1. **DAZ Studio Default**: `C:\Users\Public\Documents\My DAZ 3D Library`
2. **Registry entries**: Libraries registered in Windows Registry by DAZ Studio
3. **Daz Connect Libraries**: If DAZ Connect is installed

### Adding Libraries Manually

1. Open Settings (⚙️ icon)
2. Click "Add Library"
3. Select the folder containing DAZ content

A valid DAZ library typically contains:
- `data/` folder
- `Runtime/` folder
- `People/` or `Props/` folders

### Setting Default Library

Click the ⭐ icon next to a library to set it as the default import destination.

## External Tools

### RAR Support

For RAR archive extraction, install one of:

- **WinRAR**: https://www.rarlab.com/
- **UnRAR**: Command-line version

FileManagerDaz auto-detects UnRAR in:
- `C:\Program Files\WinRAR\UnRAR.exe`
- `C:\Program Files (x86)\WinRAR\UnRAR.exe`
- System PATH

### 7z Support

7z extraction is built-in (via `sevenz-rust`). External 7-Zip is optional but can be faster for large archives.

## Resetting Configuration

### Reset Settings Only

Delete the settings file:

```powershell
Remove-Item "$env:APPDATA\filemanagerdaz\FileManagerDaz\data\settings.json"
```

Next launch will recreate default settings.

### Full Reset (Including Database)

Delete the entire application data folder:

```powershell
Remove-Item -Recurse "$env:APPDATA\filemanagerdaz\FileManagerDaz"
```

⚠️ **Warning**: This deletes all product records and import history.

### Clear Temporary Files

```powershell
Remove-Item -Recurse "$env:TEMP\FileManagerDaz"
```

## Troubleshooting

### Settings Not Saving

1. Check write permissions to `%APPDATA%`
2. Ensure the app is closed before manual edits
3. Verify JSON syntax is valid

### Database Locked

1. Close all instances of FileManagerDaz
2. Check for zombie processes: `taskkill /f /im FileManagerDaz.exe`

### Libraries Not Detected

1. Ensure DAZ Studio has been run at least once
2. Manually add libraries in Settings
3. Verify folder permissions

### Logs

Enable developer logging in Settings to generate detailed extraction logs:

```json
{
  "dev_log_extraction_timings": true
}
```

Logs are written to:
```
%APPDATA%\filemanagerdaz\FileManagerDaz\data\logs\
```
