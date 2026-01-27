/**
 * # Import Tasks API Client
 *
 * Pure API layer for import task operations.
 *
 * ## Overview
 *
 * Encapsulates all Tauri `invoke` calls for import task persistence.
 * This module is stateless and has no side effects - it simply wraps
 * the backend commands with TypeScript types.
 *
 * ## Backend Commands Mapping
 *
 * | Function            | Backend Command                          |
 * |---------------------|------------------------------------------|
 * | listImportTasks     | list_import_tasks                        |
 * | listRecentTasks     | list_recent_import_tasks                 |
 * | getImportTask       | get_import_task                          |
 * | createImportTask    | create_import_task                       |
 * | updateTaskStatus    | update_import_task_status                |
 * | completeTask        | complete_import_task                     |
 * | failTask            | fail_import_task                         |
 * | prepareTaskRetry    | prepare_task_retry                       |
 * | deleteImportTask    | delete_import_task                       |
 * | cleanupOldTasks     | cleanup_old_import_tasks                 |
 * | processSourceWithEvents | process_source_recursive_with_events_cmd |
 * | processSource       | process_source_recursive_cmd             |
 *
 * ## Usage
 *
 * ```typescript
 * import * as api from '$lib/api/import-tasks';
 *
 * // Create and process a task
 * await api.createImportTask(id, path, name, targetLibrary);
 * const result = await api.processSourceWithEvents(id, path, 5);
 * await api.completeTask(id, dest, files, size, type);
 * ```
 */
import { invoke } from '@tauri-apps/api/core';
import type { ApiResponse, RecursiveExtractResult } from '$lib/api/commands';
import type { PersistedImportTask, ImportStatus } from '$lib/stores/imports';

// =============================================================================
// TASK LISTING
// =============================================================================

/**
 * Lists all import tasks from the database.
 * @returns All persisted tasks, ordered by creation date descending.
 */
export async function listImportTasks(): Promise<ApiResponse<PersistedImportTask[]>> {
  return invoke<ApiResponse<PersistedImportTask[]>>('list_import_tasks');
}

/**
 * Lists recent import tasks within a time window.
 * @param days - Number of days to look back.
 * @returns Tasks created within the specified time window.
 */
export async function listRecentTasks(days: number): Promise<ApiResponse<PersistedImportTask[]>> {
  return invoke<ApiResponse<PersistedImportTask[]>>('list_recent_import_tasks', { days });
}

/**
 * Retrieves a single task by ID.
 * @param id - UUID of the task.
 * @returns The task if found, null otherwise.
 */
export async function getImportTask(id: string): Promise<ApiResponse<PersistedImportTask | null>> {
  return invoke<ApiResponse<PersistedImportTask | null>>('get_import_task', { id });
}

// =============================================================================
// TASK LIFECYCLE
// =============================================================================

/**
 * Creates a new import task in the database.
 * @param id - UUID for the new task.
 * @param sourcePath - Path to the source archive or directory.
 * @param name - Display name (usually the filename).
 * @param targetLibrary - Optional target DAZ library path.
 */
export async function createImportTask(
  id: string,
  sourcePath: string,
  name: string,
  targetLibrary: string | null
): Promise<void> {
  await invoke('create_import_task', { id, sourcePath, name, targetLibrary });
}

/**
 * Updates the status of a task.
 * @param id - UUID of the task.
 * @param status - New status value.
 */
export async function updateTaskStatus(id: string, status: ImportStatus): Promise<void> {
  await invoke('update_import_task_status', { id, status });
}

/**
 * Marks a task as successfully completed.
 *
 * Also moves source archives to trash if the setting is enabled.
 *
 * @param id - UUID of the task.
 * @param destination - Final destination path.
 * @param filesCount - Number of files extracted.
 * @param totalSize - Total size in bytes.
 * @param contentType - Detected content type (e.g., "Character", "Clothing").
 * @param sourceArchivePaths - Paths to move to trash if setting enabled.
 */
export async function completeTask(
  id: string,
  destination: string,
  filesCount: number,
  totalSize: number,
  contentType: string | null,
  sourceArchivePaths?: string[]
): Promise<void> {
  await invoke('complete_import_task', { 
    id, 
    destination, 
    filesCount, 
    totalSize, 
    contentType,
    sourceArchivePaths: sourceArchivePaths ?? null
  });
}

/**
 * Marks a task as failed with an error message.
 * @param id - UUID of the task.
 * @param error - Error message describing the failure.
 */
export async function failTask(id: string, error: string): Promise<void> {
  await invoke('fail_import_task', { id, error });
}

/**
 * Prepares a task for retry by resetting its status.
 * @param id - UUID of the task.
 * @returns True if the task was successfully prepared.
 */
export async function prepareTaskRetry(id: string): Promise<ApiResponse<boolean>> {
  return invoke<ApiResponse<boolean>>('prepare_task_retry', { id });
}

// =============================================================================
// TASK CLEANUP
// =============================================================================

/**
 * Deletes a task from the database.
 * @param id - UUID of the task to delete.
 */
export async function deleteImportTask(id: string): Promise<void> {
  await invoke('delete_import_task', { id });
}

/**
 * Cleans up old completed tasks.
 * @param days - Delete tasks older than this many days.
 * @returns Number of tasks deleted.
 */
export async function cleanupOldTasks(days: number): Promise<ApiResponse<number>> {
  return invoke<ApiResponse<number>>('cleanup_old_import_tasks', { days });
}

/**
 * Clears all completed tasks (done or error status) from the database.
 * @returns Number of tasks deleted.
 */
export async function clearCompletedTasks(): Promise<ApiResponse<number>> {
  return invoke<ApiResponse<number>>('clear_completed_import_tasks');
}

// =============================================================================
// PROCESSING
// =============================================================================

/**
 * Processes a source with recursive extraction and progress events.
 *
 * This is the main processing function that:
 * 1. Extracts archives recursively (up to maxDepth levels)
 * 2. Emits "import_step" events for UI progress
 * 3. Copies content to the DAZ library
 *
 * @param taskId - UUID of the task (for event correlation).
 * @param path - Path to the source archive or directory.
 * @param maxDepth - Maximum nesting depth for recursive extraction.
 * @returns Extraction result with statistics.
 */
export async function processSourceWithEvents(
  taskId: string,
  path: string,
  maxDepth: number
): Promise<ApiResponse<RecursiveExtractResult>> {
  return invoke<ApiResponse<RecursiveExtractResult>>(
    'process_source_recursive_with_events_cmd',
    { taskId, path, maxDepth }
  );
}

/**
 * Processes a source without progress events.
 * Simpler version for batch processing without UI feedback.
 * @param path - Path to the source archive or directory.
 * @returns Extraction result with statistics.
 */
export async function processSource(path: string): Promise<ApiResponse<RecursiveExtractResult>> {
  return invoke<ApiResponse<RecursiveExtractResult>>('process_source_recursive_cmd', {
    path,
    maxDepth: 5,
  });
}
