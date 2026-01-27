import { invoke } from '@tauri-apps/api/core';

/**
 * Cleanup temporary extracted folders
 * 
 * Removes all *_extracted folders from the temp directory.
 * Useful after crashes or interrupted batch operations.
 * 
 * @returns Number of folders cleaned up
 */
export async function cleanupTempExtractions(): Promise<number> {
  return await invoke<number>('cleanup_temp_extractions');
}

/**
 * Get checkpoint status for a batch operation
 * 
 * @param sessionId - The task ID / session ID of the batch
 * @returns Checkpoint data or null if not found
 */
export async function getCheckpointStatus(sessionId: string): Promise<CheckpointData | null> {
  return await invoke<CheckpointData | null>('get_checkpoint_status', { sessionId });
}

/**
 * Process batch with robust error handling and checkpoint support
 * 
 * Automatically handles:
 * - Cleanup of partial extractions
 * - Resume from previous crash
 * - Progress tracking with events
 * - Error isolation (failed items don't stop batch)
 * 
 * @param paths - Array of source paths to process
 * @param taskId - Optional task ID for resuming (use same ID to resume)
 * @returns Batch operation result with stats
 */
export async function processBatchRobust(
  paths: string[],
  taskId?: string
): Promise<BatchOperationResult> {
  return await invoke<BatchOperationResult>('process_batch_robust', { paths, taskId });
}

// ============================================================================
// Types
// ============================================================================

export interface CheckpointData {
  session_id: string;
  total_items: number;
  processed: string[];
  failed: FailedItem[];
  last_update: number;
}

export interface FailedItem {
  path: string;
  error: string;
  timestamp: number;
}

export interface BatchOperationResult {
  successes: BatchItemResult[];
  failures: BatchItemFailure[];
  stats: BatchStats;
}

export interface BatchItemResult {
  source_path: string;
  destination: string;
  files_count: number;
  total_size: number;
  duration_ms: number;
}

export interface BatchItemFailure {
  source_path: string;
  error: string;
  error_code: string;
  skipped: boolean;
}

export interface BatchStats {
  total_items: number;
  successful: number;
  failed: number;
  total_files: number;
  total_size_bytes: number;
  duration_seconds: number;
}

export interface BatchProgress {
  total: number;
  completed: number;
  succeeded: number;
  failed: number;
  current_item?: string;
  eta_seconds?: number;
}

// ============================================================================
// Usage Examples
// ============================================================================

/**
 * Example: Process large batch with checkpoint support
 * 
 * ```typescript
 * import { processBatchRobust } from '$lib/api/checkpoint';
 * import { listen } from '@tauri-apps/api/event';
 * 
 * const taskId = 'dazfinder_import_2024';
 * 
 * // Listen for progress
 * const unlisten = await listen(`batch-progress-${taskId}`, (event) => {
 *   const progress = event.payload as BatchProgress;
 *   console.log(`${progress.completed}/${progress.total} (${progress.succeeded} ✓, ${progress.failed} ✗)`);
 * });
 * 
 * try {
 *   const result = await processBatchRobust(paths, taskId);
 *   console.log(`Complete: ${result.stats.successful}/${result.stats.total_items}`);
 *   
 *   if (result.failures.length > 0) {
 *     console.error('Failed items:', result.failures);
 *   }
 * } finally {
 *   unlisten();
 * }
 * ```
 */

/**
 * Example: Check if batch can be resumed
 * 
 * ```typescript
 * import { getCheckpointStatus } from '$lib/api/checkpoint';
 * 
 * const checkpoint = await getCheckpointStatus('dazfinder_import_2024');
 * 
 * if (checkpoint) {
 *   const percent = (checkpoint.processed.length / checkpoint.total_items * 100).toFixed(1);
 *   console.log(`Found checkpoint: ${percent}% complete`);
 *   console.log(`Processed: ${checkpoint.processed.length}`);
 *   console.log(`Failed: ${checkpoint.failed.length}`);
 *   
 *   // Resume by using the same taskId
 *   await processBatchRobust(allPaths, 'dazfinder_import_2024');
 * } else {
 *   console.log('No checkpoint found, starting fresh');
 * }
 * ```
 */

/**
 * Example: Cleanup temp directory before processing
 * 
 * ```typescript
 * import { cleanupTempExtractions } from '$lib/api/checkpoint';
 * 
 * // Manual cleanup (automatic in process_batch_robust)
 * const cleaned = await cleanupTempExtractions();
 * console.log(`Cleaned ${cleaned} temporary folders`);
 * ```
 */
