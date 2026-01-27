# Checkpoint & Recovery System

## Overview

The checkpoint system enables crash recovery for large batch operations. It automatically:
- **Tracks progress** during batch processing
- **Saves checkpoints** to disk after each item
- **Resumes from last checkpoint** if the app crashes or is restarted
- **Cleans up** partial extractions before starting

## Architecture

### Components

```
checkpoint.rs
├── Checkpoint           → State tracking (processed items, failures)
├── cleanup_extracted_folders() → Removes *_extracted temp folders
└── FailedItem          → Error details for failed items

batch.rs
└── RobustBatchProcessor
    ├── with_checkpoint()    → Enable resume capability
    └── with_cleanup()       → Auto-cleanup temp directory
```

### Data Flow

```
User starts batch
    ↓
Cleanup *_extracted folders ← Remove partial extractions
    ↓
Load checkpoint (if exists) ← Resume from crash
    ↓
Filter already processed   ← Skip completed items
    ↓
Process remaining items
    ↓
Save checkpoint after each ← Incremental progress
    ↓
Delete checkpoint on success
```

## Usage

### Basic Usage (with checkpoint)

The `process_batch_robust` command automatically enables checkpointing:

```typescript
// Frontend
await invoke('process_batch_robust', {
  paths: ['/path/to/archives/...'],
  taskId: 'my_import_session'
});

// Listen for progress
listen(`batch-progress-my_import_session`, (event) => {
  console.log(`${event.completed}/${event.total} processed`);
  console.log(`Succeeded: ${event.succeeded}, Failed: ${event.failed}`);
});
```

### Checkpoint Location

Checkpoints are stored at:
```
%TEMP%/FileManagerDaz/checkpoints/{session_id}.json
```

Example: `C:\Users\Yanis\AppData\Local\Temp\FileManagerDaz\checkpoints\batch_1234567890.json`

### Checkpoint Structure

```json
{
  "session_id": "batch_1234567890",
  "total_items": 2785,
  "processed": [
    "C:/path/archive1.zip",
    "C:/path/archive2.7z"
  ],
  "failed": [
    {
      "path": "C:/path/corrupted.rar",
      "error": "RAR extraction failed: corrupted header",
      "timestamp": 1702346567
    }
  ],
  "last_update": 1702346890
}
```

## Features

### 1. Automatic Cleanup

Before processing, removes all `*_extracted` folders from temp directory:

```rust
cleanup_extracted_folders(&temp_dir)?;
// Removes: archive1_extracted/, archive2_extracted/, etc.
```

**Why**: Previous crashes leave hundreds of extracted folders consuming disk space.

### 2. Resume from Crash

If app crashes at item 538/3323:

```rust
// On restart, automatically:
1. Load checkpoint → 538 already processed
2. Skip 538 items → Start at item 539
3. Continue processing → 2785 remaining
```

### 3. Progress Tracking

Checkpoint tracks:
- ✅ **Processed items** (canonical paths)
- ❌ **Failed items** (with error details)
- 📊 **Progress percentage** (`processed/total * 100`)
- 🕐 **Last update timestamp**

### 4. Error Isolation

Failed items don't stop the batch:

```rust
// Item 42 fails → saved to checkpoint.failed[]
// Processing continues with item 43
// Checkpoint persists failure details for review
```

## API

### RobustBatchProcessor

```rust
use crate::core::extractor::RobustBatchProcessor;
use crate::config::SETTINGS;

let settings = SETTINGS.read().unwrap();
let config = settings.to_resilience_config();

let result = RobustBatchProcessor::new(config)
    .with_checkpoint(checkpoint_dir, session_id) // Enable resume
    .with_cleanup(true)                          // Auto-cleanup temp
    .with_progress(|progress| {
        // Called after each item
        println!("{}/{} items", progress.completed, progress.total);
    })
    .process_batch(paths)?;
```

### Checkpoint Methods

```rust
// Create new checkpoint
let mut checkpoint = Checkpoint::new("session_id".into(), 1000);

// Mark items processed
checkpoint.mark_processed(&path);
checkpoint.mark_failed(&path, "error message".into());

// Check status
if checkpoint.is_processed(&path) {
    println!("Already done, skip!");
}

// Calculate remaining
let remaining = checkpoint.get_remaining(&all_paths);

// Persistence
checkpoint.save(&checkpoint_dir)?;
let loaded = Checkpoint::load(&checkpoint_dir, "session_id")?;
checkpoint.delete(&checkpoint_dir)?; // Cleanup after success
```

## Configuration

### AppSettings

```rust
pub struct AppSettings {
    // ... existing fields
    
    // Checkpoint settings (future enhancement)
    pub checkpoint_interval: u32,        // Save every N items (default: 1)
    pub checkpoint_auto_cleanup: bool,   // Delete old checkpoints (default: true)
    pub checkpoint_retention_days: u32,  // Keep checkpoints N days (default: 7)
}
```

## Scenarios

### Scenario 1: Normal Batch (No Crash)

```
1. Start: 3323 items
2. Cleanup: Remove 538 *_extracted folders
3. Process: All 3323 items
4. Success: Delete checkpoint
5. Result: Clean temp directory
```

### Scenario 2: Crash at 538/3323

```
1. Start: 3323 items
2. Process: Items 1-538
3. CRASH at item 538
4. Checkpoint saved: 538 processed

--- App Restart ---

5. Load checkpoint: 538 already done
6. Cleanup: Remove 538 *_extracted folders
7. Resume: Process items 539-3323
8. Success: Delete checkpoint
```

### Scenario 3: Multiple Crashes

```
First run: Process 500 items → CRASH
Second run: Resume from 500 → Process 300 more → CRASH
Third run: Resume from 800 → Complete 2523 remaining → SUCCESS
```

Each restart automatically resumes from the last saved checkpoint.

## Monitoring

### Progress Events

Frontend receives real-time updates:

```typescript
listen(`batch-progress-${taskId}`, (event) => {
  const { total, completed, succeeded, failed, current_item } = event.payload;
  
  console.log(`Progress: ${completed}/${total} (${(completed/total*100).toFixed(1)}%)`);
  console.log(`Success: ${succeeded}, Failed: ${failed}`);
  console.log(`Current: ${current_item}`);
});
```

### Checkpoint Inspection

Manually check checkpoint status:

```powershell
# View checkpoint
cat "$env:TEMP\FileManagerDaz\checkpoints\batch_1234567890.json"

# List all checkpoints
ls "$env:TEMP\FileManagerDaz\checkpoints\"

# Count processed items
(cat checkpoint.json | ConvertFrom-Json).processed.Count
```

## Performance

### Checkpoint Overhead

- **Save frequency**: After each item (~1ms per save)
- **I/O impact**: Minimal (JSON write to temp disk)
- **Memory**: O(n) for processed paths HashSet

### Cleanup Performance

- **538 folders**: ~2-5 seconds
- **Parallel**: Could be improved with rayon (future)
- **Bottleneck**: Disk I/O for `fs::remove_dir_all`

## Troubleshooting

### Issue: Checkpoint not resuming

**Check**:
```rust
// Verify checkpoint exists
let checkpoint_file = format!("{}/{}.json", checkpoint_dir, session_id);
println!("Looking for: {}", checkpoint_file);
```

**Solution**: Ensure `task_id` is consistent between runs.

### Issue: Disk space full

**Cleanup manually**:
```powershell
# Remove extracted folders
Remove-Item "$env:TEMP\FileManagerDaz\downloads_import\*_extracted" -Recurse -Force

# Remove old checkpoints
Remove-Item "$env:TEMP\FileManagerDaz\checkpoints\*.json"
```

### Issue: Corrupted checkpoint

**Symptoms**: JSON parse error on load

**Solution**:
```powershell
# Delete corrupted checkpoint
Remove-Item "$env:TEMP\FileManagerDaz\checkpoints\batch_xyz.json"

# Restart batch (will create new checkpoint)
```

## Future Enhancements

### Planned Features

1. **Checkpoint Versioning**: Migrate old checkpoint formats
2. **Partial Resume**: Resume from specific item (not just skip all processed)
3. **Checkpoint Compression**: Store large checkpoints as .gz
4. **Cloud Sync**: Backup checkpoints to cloud storage
5. **Multi-Session**: Handle multiple concurrent batches
6. **Checkpoint UI**: Show checkpoint history in frontend

### Advanced Options

```rust
// Future API
RobustBatchProcessor::new(config)
    .with_checkpoint_interval(10)           // Save every 10 items
    .with_checkpoint_compression(true)      // Use gzip
    .with_checkpoint_cloud_sync(s3_config)  // Backup to S3
    .process_batch(paths)?;
```

## Testing

### Unit Tests

```rust
#[test]
fn test_checkpoint_lifecycle() {
    let checkpoint = Checkpoint::new("test".into(), 10);
    checkpoint.mark_processed(&path);
    assert!(checkpoint.is_processed(&path));
    checkpoint.save(&dir)?;
    
    let loaded = Checkpoint::load(&dir, "test")?;
    assert_eq!(loaded.processed.len(), 1);
}
```

### Integration Test

```bash
# 1. Start batch with 100 items
# 2. Kill process at item 50
# 3. Restart
# 4. Verify: 50 items skipped, 50 remaining processed
```

## See Also

- [RESILIENCE.md](./RESILIENCE.md) - Retry and timeout configuration
- [../config/settings.rs](../src-tauri/src/config/settings.rs) - AppSettings structure
- [../commands/archive.rs](../src-tauri/src/commands/archive.rs) - `process_batch_robust` command

## Summary

The checkpoint system provides:

✅ **Crash recovery** - Resume from any point  
✅ **Auto-cleanup** - Remove partial extractions  
✅ **Progress persistence** - Never lose work  
✅ **Error tracking** - Review failures  
✅ **Disk space management** - Clean temp directory  

Perfect for processing **thousands of archives** over multiple sessions.
