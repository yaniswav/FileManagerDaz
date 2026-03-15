/**
 * # Import Task Store
 *
 * Central state management for import tasks in FileManagerDaz.
 *
 * ## Architecture
 *
 * ```
 * UI Components
 *      ↓ call
 * Orchestration Functions (processMultipleSources, retryTask, etc.)
 *      ↓ update
 * Svelte Store (importsStore)
 *      ↓ sync
 * Backend API (import-tasks.ts → Tauri commands)
 * ```
 *
 * ## Key Concepts
 *
 * - **ImportTask**: Local view model with UI state (steps, currentStep)
 * - **PersistedImportTask**: Backend database record
 * - **Derived stores**: processingTasks, completedTasks, retryableTasks
 * - **Event listener**: Receives "import_step" events from backend
 *
 * ## Usage
 *
 * ```typescript
 * import { importsStore, processMultipleSources, retryTask } from '$lib/stores/imports';
 *
 * // Start imports
 * await processMultipleSources(['/path/to/archive.zip']);
 *
 * // Subscribe to state
 * importsStore.subscribe(tasks => console.log(tasks));
 *
 * // Retry failed task
 * await retryTask(taskId);
 * ```
 */
import { writable, derived, get } from 'svelte/store';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type { RecursiveExtractResult, ContentType } from '$lib/api/commands';
import * as api from '$lib/api/import-tasks';
import { getIntlLocale } from '$lib/i18n';
import { notify } from '$lib/stores/notifications';

// =============================================================================
// TYPES
// =============================================================================

/** Import task lifecycle status. */
export type ImportStatus = 'pending' | 'processing' | 'done' | 'error' | 'interrupted';

/** A single step in the import process log. */
export interface ImportStep {
  /** Human-readable step description. */
  message: string;
  /** Optional additional details. */
  details: string | null;
  /** Unix timestamp in milliseconds. */
  timestamp: number;
  /** Formatted time string (HH:MM:SS) for display. */
  time: string;
}

/** Import task as stored in the backend database. */
export interface PersistedImportTask {
  id: string;
  sourcePath: string;
  name: string;
  status: ImportStatus;
  destination: string | null;
  errorMessage: string | null;
  filesCount: number | null;
  totalSize: number | null;
  contentType: string | null;
  startedAt: number;
  completedAt: number | null;
  targetLibrary: string | null;
}

/** Import task view model with UI-specific state. */
export interface ImportTask {
  /** Unique task identifier (UUID). */
  id: string;
  /** Source path (archive or directory). */
  path: string;
  /** Display name. */
  name: string;
  /** Current lifecycle status. */
  status: ImportStatus;
  /** Extraction result (on success). */
  result: RecursiveExtractResult | null;
  /** Error message (on failure). */
  error: string | null;
  /** Task start timestamp. */
  startedAt: number;
  /** Task completion timestamp. */
  completedAt: number | null;
  /** Current processing step (for UI feedback). */
  currentStep: string | null;
  /** Log of all processing steps. */
  steps: ImportStep[];
  /** Target DAZ library path. */
  targetLibrary: string | null;
}

/** Event payload received from backend via Tauri events. */
interface ImportStepEvent {
  taskId: string;
  message: string;
  details: string | null;
  timestamp: number;
}

// =============================================================================
// HELPERS
// =============================================================================

/** Formats a timestamp as HH:MM:SS for display. */
function formatTime(timestamp: number): string {
  const date = new Date(timestamp);
  return date.toLocaleTimeString(getIntlLocale(), {
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
  });
}

/** Converts a backend PersistedImportTask to a local ImportTask view model. */
function persistedToLocal(persistedTask: PersistedImportTask): ImportTask {
  return {
    id: persistedTask.id,
    path: persistedTask.sourcePath,
    name: persistedTask.name,
    status: persistedTask.status,
    result: persistedTask.destination
      ? {
          source_path: persistedTask.sourcePath,
          destination: persistedTask.destination,
          total_files: persistedTask.filesCount ?? 0,
          total_size: persistedTask.totalSize ?? 0,
          archive_format: null,
          nested_archives: [],
          analysis: persistedTask.contentType
            ? {
                content_type: persistedTask.contentType as ContentType,
                is_daz_content: true,
                suggested_tags: [],
                daz_file_count: 0,
                texture_count: 0,
                daz_folders: [],
                wrapper_folder: null,
                detected_figures: [],
                warnings: [],
              }
            : null,
          max_depth_reached: 0,
          moved_to_library: !!persistedTask.destination,
          // Archives already trashed during initial import
          source_archive_paths: [],
        }
      : null,
    error: persistedTask.errorMessage,
    startedAt: persistedTask.startedAt,
    completedAt: persistedTask.completedAt,
    currentStep: null,
    steps: [],
    targetLibrary: persistedTask.targetLibrary,
  };
}

// =============================================================================
// STORE IMPLEMENTATION
// =============================================================================

function createImportsStore() {
  const { subscribe, set, update } = writable<ImportTask[]>([]);
  let initialized = false;

  return {
    subscribe,

    /**
     * Charge les tâches depuis la base de données
     * À appeler au démarrage de l'application
     */
    async loadFromDatabase(): Promise<void> {
      if (initialized) return;

      try {
        const response = await api.listImportTasks();
        if (response.ok && response.data) {
          const tasks = response.data.map(persistedToLocal);
          set(tasks);
          console.log(`[ImportsStore] Loaded ${tasks.length} tasks from database`);
        }
        initialized = true;
      } catch (err) {
        console.error('[ImportsStore] Failed to load tasks:', err);
      }
    },

    /**
     * Creates new import tasks for the given source paths.
     * Persists to database immediately.
     */
    async addTasks(paths: string[], targetLibrary?: string): Promise<ImportTask[]> {
      const now = Date.now();
      const newTasks: ImportTask[] = [];

      for (let i = 0; i < paths.length; i++) {
        const sourcePath = paths[i];
        const taskId = `${now}-${i}`;
        const displayName = sourcePath.split(/[\\/]/).pop() || sourcePath;

        // Persist to database via API — skip task if backend fails
        try {
          await api.createImportTask(taskId, sourcePath, displayName, targetLibrary ?? null);
        } catch (err) {
          console.error('[ImportsStore] Failed to persist task, skipping:', sourcePath, err);
          continue;
        }

        const task: ImportTask = {
          id: taskId,
          path: sourcePath,
          name: displayName,
          status: 'pending',
          result: null,
          error: null,
          startedAt: now,
          completedAt: null,
          currentStep: null,
          steps: [],
          targetLibrary: targetLibrary ?? null,
        };
        newTasks.push(task);
      }

      if (newTasks.length > 0) {
        update((tasks) => [...newTasks, ...tasks]);
      }
      return newTasks;
    },

    /**
     * Updates the status of a task.
     * Syncs to database asynchronously.
     */
    updateStatus(taskId: string, status: ImportStatus) {
      api.updateTaskStatus(taskId, status).catch((err) =>
        console.error('[ImportsStore] Failed to update status:', err)
      );

      update((tasks) => tasks.map((task) => (task.id === taskId ? { ...task, status } : task)));
    },

    /**
     * Adds a processing step to a task's log.
     */
    addStep(taskId: string, message: string, details: string | null, timestamp: number) {
      const step: ImportStep = {
        message,
        details,
        timestamp,
        time: formatTime(timestamp),
      };

      update((tasks) =>
        tasks.map((task) =>
          task.id === taskId
            ? {
                ...task,
                currentStep: message,
                steps: [...task.steps, step],
              }
            : task
        )
      );
    },

    /**
     * Marks a task as completed with the given result.
     * Syncs to database and triggers archive trash if enabled.
     */
    async setResult(taskId: string, result: RecursiveExtractResult) {
      try {
        await api.completeTask(
          taskId,
          result.destination,
          result.total_files,
          result.total_size,
          result.analysis?.content_type ?? null,
          result.source_archive_paths
        );
      } catch (err) {
        console.error('[ImportsStore] Failed to persist result:', err);
      }

      update((tasks) =>
        tasks.map((task) =>
          task.id === taskId
            ? {
                ...task,
                status: 'done' as ImportStatus,
                result,
                completedAt: Date.now(),
                currentStep: null,
              }
            : task
        )
      );
    },

    /**
     * Marks a task as failed with the given error message.
     */
    async setError(taskId: string, errorMessage: string) {
      try {
        await api.failTask(taskId, errorMessage);
      } catch (err) {
        console.error('[ImportsStore] Failed to persist error:', err);
      }

      const now = Date.now();
      update((tasks) =>
        tasks.map((task) =>
          task.id === taskId
            ? {
                ...task,
                status: 'error' as ImportStatus,
                error: errorMessage,
                completedAt: now,
                currentStep: null,
                steps: [
                  ...task.steps,
                  {
                    message: 'Error',
                    details: errorMessage,
                    timestamp: now,
                    time: formatTime(now),
                  },
                ],
              }
            : task
        )
      );
    },

    /**
     * Prepares a task for retry by resetting its status.
     * Returns true if successful.
     */
    async prepareRetry(taskId: string): Promise<boolean> {
      try {
        const response = await api.prepareTaskRetry(taskId);
        if (response.ok && response.data) {
          update((tasks) =>
            tasks.map((task) =>
              task.id === taskId
                ? {
                    ...task,
                    status: 'pending' as ImportStatus,
                    error: null,
                    result: null,
                    completedAt: null,
                    currentStep: null,
                    steps: [],
                    startedAt: Date.now(),
                  }
                : task
            )
          );
          return true;
        }
        return false;
      } catch (err) {
        console.error('[ImportsStore] Failed to prepare retry:', err);
        return false;
      }
    },

    /**
     * Removes a task from both store and database.
     */
    async removeTask(taskId: string) {
      try {
        await api.deleteImportTask(taskId);
      } catch (err) {
        console.error('[ImportsStore] Failed to delete task:', err);
      }

      update((tasks) => tasks.filter((task) => task.id !== taskId));
    },

    /**
     * Removes completed tasks (done or error) from both local store and database.
     * Returns the number of tasks deleted from database.
     */
    async clearCompleted(): Promise<number> {
      // First, delete from database
      let deletedCount = 0;
      try {
        const response = await api.clearCompletedTasks();
        if (response.ok && response.data) {
          deletedCount = response.data;
          console.log(`[ImportsStore] Cleared ${deletedCount} completed tasks from database`);
        }
      } catch (err) {
        console.error('[ImportsStore] Failed to clear completed tasks from database:', err);
      }

      // Then, update local store
      update((tasks) =>
        tasks.filter((task) => task.status === 'pending' || task.status === 'processing')
      );

      return deletedCount;
    },

    /**
     * Deletes tasks older than the specified number of days from database.
     * Returns the count of deleted tasks.
     */
    async cleanupOldTasks(days: number): Promise<number> {
      try {
        const response = await api.cleanupOldTasks(days);
        return response.ok && response.data ? response.data : 0;
      } catch (err) {
        console.error('[ImportsStore] Failed to cleanup:', err);
        return 0;
      }
    },

    /**
     * Resets the store to initial state.
     */
    reset() {
      set([]);
      initialized = false;
    },

    /**
     * Retrieves a task by its ID.
     */
    getTask(taskId: string): ImportTask | undefined {
      return get({ subscribe }).find((task) => task.id === taskId);
    },

    /**
     * Returns all tasks that can be retried (error or interrupted).
     */
    getRetryableTasks(): ImportTask[] {
      return get({ subscribe }).filter(
        (task) => task.status === 'error' || task.status === 'interrupted'
      );
    },
  };
}

export const importsStore = createImportsStore();

// =============================================================================
// DERIVED STORES
// =============================================================================

/** Tasks currently being processed. */
export const processingTasks = derived(importsStore, ($tasks) =>
  $tasks.filter((t) => t.status === 'pending' || t.status === 'processing')
);

/** Successfully completed tasks. */
export const completedTasks = derived(importsStore, ($tasks) =>
  $tasks.filter((t) => t.status === 'done')
);

/** Tasks that failed with an error. */
export const errorTasks = derived(importsStore, ($tasks) =>
  $tasks.filter((t) => t.status === 'error')
);

/** Tasks interrupted by app restart. */
export const interruptedTasks = derived(importsStore, ($tasks) =>
  $tasks.filter((t) => t.status === 'interrupted')
);

/** Tasks that can be retried (error + interrupted). */
export const retryableTasks = derived(importsStore, ($tasks) =>
  $tasks.filter((t) => t.status === 'error' || t.status === 'interrupted')
);

/** Whether any task is currently processing. */
export const isProcessing = derived(importsStore, ($tasks) =>
  $tasks.some((t) => t.status === 'pending' || t.status === 'processing')
);

/** Count of tasks currently processing. */
export const processingCount = derived(processingTasks, ($tasks) => $tasks.length);

// =============================================================================
// TAURI EVENT LISTENER
// =============================================================================

let stepListenerUnlisten: UnlistenFn | null = null;

/**
 * Initializes the Tauri event listener for import step events.
 * Should be called once at application startup.
 */
export async function initStepListener(): Promise<void> {
  if (stepListenerUnlisten) return; // Already initialized

  try {
    stepListenerUnlisten = await listen<ImportStepEvent>('import_step', (event) => {
      const { taskId, message, details, timestamp } = event.payload;
      console.log(`[ImportStep] ${taskId}: ${message}`, details);
      importsStore.addStep(taskId, message, details, timestamp);
    });
    console.log('[ImportsStore] Step listener initialized');
  } catch (err) {
    console.error('[ImportsStore] Failed to init step listener:', err);
  }
}

/**
 * Stops listening to Tauri events.
 */
export function stopStepListener(): void {
  stepListenerUnlisten?.();
  stepListenerUnlisten = null;
}

// =============================================================================
// ORCHESTRATION FUNCTIONS
// =============================================================================

/**
 * Deduplicates multi-part archive paths.
 *
 * When a user drops multiple parts of the same split archive
 * (e.g., file.part1.rar, file.part2.rar, file.z01, file.zip),
 * only keep the primary part to avoid duplicate processing.
 */
function deduplicateMultiParts(paths: string[]): string[] {
  const seen = new Set<string>();
  const result: string[] = [];

  for (const p of paths) {
    const name = p.split(/[\\/]/).pop()?.toLowerCase() ?? '';

    // RAR .partN.rar: group by base name, keep only part1
    const rarPartMatch = name.match(/^(.+)\.part(\d+)\.rar$/);
    if (rarPartMatch) {
      const base = rarPartMatch[1];
      const key = `rar-part:${base}`;
      if (!seen.has(key)) {
        seen.add(key);
        // Find part1 in the paths list, or use this one
        const part1 = paths.find(pp => {
          const n = pp.split(/[\\/]/).pop()?.toLowerCase() ?? '';
          return n.match(new RegExp(`^${base.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}\\.part0*1\\.rar$`));
        });
        result.push(part1 ?? p);
      }
      continue;
    }

    // RAR old-style .r00, .r01, etc.: skip these, keep only .rar
    if (/\.[rR]\d{2}$/.test(name)) {
      continue;
    }

    // ZIP split .z01, .z02, etc.: skip these, keep only .zip
    if (/\.[zZ]\d{2}$/.test(name)) {
      continue;
    }

    result.push(p);
  }

  return result;
}

/**
 * Processes multiple source paths sequentially.
 *
 * This is the main entry point for processing dropped archives.
 * Creates tasks, adds them to the store, and processes them one by one.
 *
 * @param paths - Array of source paths (archives or directories)
 * @param targetLibrary - Optional target DAZ library path
 * @param onProcessed - Callback for each successful import
 * @param onError - Callback for each failed import
 */
export async function processMultipleSources(
  paths: string[],
  targetLibrary?: string,
  onProcessed?: (result: RecursiveExtractResult) => void,
  onError?: (data: { path: string; message: string }) => void
): Promise<void> {
  // Ensure event listener is active
  await initStepListener();

  // Deduplicate multi-part archives: skip secondary parts (.z01, .r00, .part2.rar, etc.)
  // The backend auto-resolves to the first part, but we avoid creating duplicate tasks
  const deduplicated = deduplicateMultiParts(paths);

  const tasks = await importsStore.addTasks(deduplicated, targetLibrary);

  let succeeded = 0;
  let failed = 0;

  for (const task of tasks) {
    await processOneTask(task, onProcessed, onError);
    // Check final status
    const current = get(importsStore).find(t => t.id === task.id);
    if (current?.status === 'done') succeeded++;
    else if (current?.status === 'error') failed++;
  }

  // Batch notification (only if more than 1 task)
  if (tasks.length > 1) {
    notify.batchComplete(succeeded, failed);
  }
}

/**
 * Processes a single import task.
 * Internal function called by processMultipleSources and retryTask.
 */
async function processOneTask(
  task: ImportTask,
  onProcessed?: (result: RecursiveExtractResult) => void,
  onError?: (data: { path: string; message: string }) => void
): Promise<void> {
  console.log('[ImportsStore] Processing task:', task.path);
  importsStore.updateStatus(task.id, 'processing');

  try {
    const response = await api.processSourceWithEvents(task.id, task.path, 5);

    if (!response.ok || !response.data) {
      throw new Error(response.error?.message ?? 'Unknown error');
    }

    const result = response.data;
    console.log('[ImportsStore] Task completed:', result);

    await importsStore.setResult(task.id, result);
    notify.importComplete(task.name, result.total_files, result.total_size);
    onProcessed?.(result);
  } catch (err) {
    const errorMessage = err instanceof Error ? err.message : String(err);
    console.error('[ImportsStore] Task error:', errorMessage);

    await importsStore.setError(task.id, errorMessage);
    notify.importFailed(task.name, errorMessage);
    onError?.({ path: task.path, message: errorMessage });
  }
}

/**
 * Retries a failed or interrupted task.
 *
 * @param taskId - ID of the task to retry
 * @param onProcessed - Callback on success
 * @param onError - Callback on failure
 */
export async function retryTask(
  taskId: string,
  onProcessed?: (result: RecursiveExtractResult) => void,
  onError?: (data: { path: string; message: string }) => void
): Promise<void> {
  // Reset task status in backend and local store
  const prepared = await importsStore.prepareRetry(taskId);
  if (!prepared) {
    console.error('[ImportsStore] Could not prepare task for retry');
    return;
  }

  const task = importsStore.getTask(taskId);
  if (!task) return;

  await processOneTask(task, onProcessed, onError);
}

/**
 * Retries all failed and interrupted tasks.
 */
export async function retryAllFailed(
  onProcessed?: (result: RecursiveExtractResult) => void,
  onError?: (data: { path: string; message: string }) => void
): Promise<void> {
  const retryable = importsStore.getRetryableTasks();

  for (const task of retryable) {
    await retryTask(task.id, onProcessed, onError);
  }
}
