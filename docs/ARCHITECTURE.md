# FileManagerDaz Architecture

This document provides a technical overview of the FileManagerDaz application architecture.

## Overview

FileManagerDaz is a desktop application built with:

- **Backend**: Rust + Tauri v2
- **Frontend**: Svelte 5 + TypeScript + Vite
- **Database**: SQLite (via rusqlite)
- **Configuration**: JSON files

```
┌─────────────────────────────────────────────────────────────────┐
│                        FileManagerDaz                           │
├─────────────────────────────────────────────────────────────────┤
│  ┌─────────────────────────────────────────────────────────┐   │
│  │                   Frontend (Svelte)                      │   │
│  │  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌─────────┐  │   │
│  │  │ DropZone │  │ Settings │  │ Products │  │ Tools   │  │   │
│  │  └────┬─────┘  └────┬─────┘  └────┬─────┘  └────┬────┘  │   │
│  │       │             │             │             │        │   │
│  │  ┌────▼─────────────▼─────────────▼─────────────▼────┐  │   │
│  │  │              Stores (imports.ts)                   │  │   │
│  │  └────────────────────┬──────────────────────────────┘  │   │
│  │                       │                                  │   │
│  │  ┌────────────────────▼──────────────────────────────┐  │   │
│  │  │           API Layer (commands.ts)                  │  │   │
│  │  └────────────────────┬──────────────────────────────┘  │   │
│  └───────────────────────┼──────────────────────────────────┘   │
│                          │ Tauri invoke()                       │
│  ┌───────────────────────▼──────────────────────────────────┐   │
│  │                   Backend (Rust)                          │   │
│  │  ┌──────────────────────────────────────────────────┐    │   │
│  │  │              Commands (Tauri handlers)            │    │   │
│  │  │  archive.rs │ settings.rs │ products.rs │ ...    │    │   │
│  │  └──────────────────────┬───────────────────────────┘    │   │
│  │                         │                                 │   │
│  │  ┌──────────────────────▼───────────────────────────┐    │   │
│  │  │                   Core Logic                      │    │   │
│  │  │  ┌───────────┐  ┌───────────┐  ┌────────────┐   │    │   │
│  │  │  │ Extractor │  │ Analyzer  │  │ Destination│   │    │   │
│  │  │  └───────────┘  └───────────┘  └────────────┘   │    │   │
│  │  └──────────────────────┬───────────────────────────┘    │   │
│  │                         │                                 │   │
│  │  ┌──────────────────────▼───────────────────────────┐    │   │
│  │  │              Persistence Layer                    │    │   │
│  │  │  ┌─────────┐  ┌─────────┐  ┌─────────────────┐  │    │   │
│  │  │  │   DB    │  │ Config  │  │   Filesystem    │  │    │   │
│  │  │  │ SQLite  │  │  JSON   │  │   (Libraries)   │  │    │   │
│  │  │  └─────────┘  └─────────┘  └─────────────────┘  │    │   │
│  │  └──────────────────────────────────────────────────┘    │   │
│  └──────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────┘
```

## Backend Architecture

### Module Structure

```
src-tauri/src/
├── main.rs              # Application entry point
├── lib.rs               # Tauri plugin configuration
├── error.rs             # Error types and API response wrapper
├── core/                # Business logic
│   ├── mod.rs
│   ├── extractor/       # Archive extraction
│   │   ├── mod.rs
│   │   ├── zip.rs       # ZIP extraction
│   │   ├── rar.rs       # RAR extraction (external unrar)
│   │   ├── sevenzip.rs  # 7z extraction (sevenz-rust)
│   │   ├── recursive.rs # Nested archive handling
│   │   └── utils.rs     # File utilities
│   ├── analyzer.rs      # DAZ content detection
│   ├── destination.rs   # Smart destination proposal
│   ├── maintenance.rs   # Library cleanup tools
│   └── watcher.rs       # Folder watching
├── commands/            # Tauri command handlers
│   ├── mod.rs
│   ├── archive.rs       # Import commands
│   ├── settings.rs      # Configuration commands
│   ├── products.rs      # Product CRUD
│   └── import_tasks.rs  # Task management
├── db/                  # Database operations
│   ├── mod.rs
│   ├── repository.rs    # Product repository
│   └── import_tasks.rs  # Task persistence
└── config/              # Settings management
    ├── mod.rs
    └── settings.rs      # AppSettings struct
```

### Import Pipeline

```
┌──────────────┐
│ User drops   │
│ archive(s)   │
└──────┬───────┘
       │
       ▼
┌──────────────────────────────────────────────────────────────┐
│ 1. TASK CREATION                                              │
│    - Generate UUID                                            │
│    - Persist to import_tasks table                            │
│    - Status: pending                                          │
└──────────────────────────────────────────────────────────────┘
       │
       ▼
┌──────────────────────────────────────────────────────────────┐
│ 2. EXTRACTION (recursive)                                     │
│    - Extract to temp directory                                │
│    - Detect format (ZIP/RAR/7z)                              │
│    - Handle nested archives recursively                       │
│    - Emit progress events                                     │
│    - Status: processing                                       │
└──────────────────────────────────────────────────────────────┘
       │
       ▼
┌──────────────────────────────────────────────────────────────┐
│ 3. ANALYSIS                                                   │
│    - Scan for DAZ markers (data/, Runtime/, etc.)            │
│    - Detect content type (Character, Clothing, etc.)         │
│    - Identify figures (Genesis 8, Genesis 9, etc.)           │
│    - Generate suggested tags                                  │
└──────────────────────────────────────────────────────────────┘
       │
       ▼
┌──────────────────────────────────────────────────────────────┐
│ 4. DESTINATION PROPOSAL                                       │
│    - Analyze library structure                                │
│    - Match content type to existing folders                   │
│    - Calculate confidence score                               │
│    - Generate alternatives                                    │
└──────────────────────────────────────────────────────────────┘
       │
       ▼
┌──────────────────────────────────────────────────────────────┐
│ 5. INSTALLATION                                               │
│    - Move/merge content to library                            │
│    - Handle duplicates                                        │
│    - Create product record                                    │
│    - Optional: trash source archive                           │
│    - Status: done                                             │
└──────────────────────────────────────────────────────────────┘
```

### Error Handling

All backend operations return `ApiResponse<T>`:

```rust
pub struct ApiResponse<T> {
    pub ok: bool,
    pub data: Option<T>,
    pub error: Option<ApiError>,
}

pub struct ApiError {
    pub code: String,      // Machine-readable: "NOT_FOUND", "ARCHIVE_ERROR"
    pub message: String,   // Human-readable description
    pub details: Option<String>,
}
```

## Frontend Architecture

### Component Hierarchy

```
+page.svelte
├── DropZone.svelte          # Main import interface
│   └── TaskCard (inline)    # Individual import task
├── Settings.svelte          # Configuration panel
│   ├── Libraries section
│   ├── External tools
│   └── Preferences
├── ProductsList.svelte      # Installed products
├── NormalizeFolder.svelte   # Batch normalization tool
└── Maintenance.svelte       # Library cleanup tools
```

### State Management

```
Svelte Stores (src/lib/stores/)
└── imports.ts
    ├── importsStore        # Main writable store
    ├── processingTasks     # Derived: currently processing
    ├── completedTasks      # Derived: finished tasks
    └── retryableTasks      # Derived: failed/interrupted
```

### API Layer

```typescript
// src/lib/api/commands.ts
export async function processSource(path: string): Promise<ExtractResult>
export async function listProducts(): Promise<Product[]>
export async function getAppConfig(): Promise<AppConfig>
// ... etc
```

## Database Schema

```sql
-- Products table
CREATE TABLE products (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    path TEXT NOT NULL,
    source_archive TEXT,
    content_type TEXT,
    installed_at TEXT NOT NULL,
    tags TEXT DEFAULT '',
    notes TEXT,
    files_count INTEGER DEFAULT 0,
    total_size INTEGER DEFAULT 0
);

-- Import tasks table
CREATE TABLE import_tasks (
    id TEXT PRIMARY KEY,
    source_path TEXT NOT NULL,
    name TEXT NOT NULL,
    status TEXT NOT NULL,
    target_library TEXT,
    destination TEXT,
    files_count INTEGER,
    total_size INTEGER,
    content_type TEXT,
    error_message TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
```

## Event System

Backend emits events for real-time UI updates:

```rust
// Emitted during extraction
app.emit("import_step", ImportStepEvent {
    task_id: "uuid",
    step: "Extracting archive...",
    detail: Some("file.zip"),
});
```

Frontend listens:

```typescript
await listen<ImportStepEvent>('import_step', (event) => {
    // Update task progress in store
});
```

## Security Considerations

- **CSP**: Configured in `tauri.conf.json`
- **File Access**: Limited to user-selected paths via Tauri dialogs
- **No Network**: Application operates entirely offline
- **Permissions**: Tauri capabilities define allowed operations
