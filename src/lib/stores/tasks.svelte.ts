/**
 * # Global Task Store
 *
 * Tracks background tasks (scan, extract, download) so every tab
 * can show live status via the StatusBar component.
 *
 * Listens to three Tauri events:
 * - `app-task-start`    → adds a new task
 * - `app-task-progress` → updates message / progress
 * - `app-task-end`      → marks as success/error, auto-removes after delay
 */

import { listen, type UnlistenFn } from '@tauri-apps/api/event';

export type TaskStatus = 'running' | 'success' | 'error';

export interface AppTask {
  id: string;
  taskType: string;
  message: string;
  progress: number | null;
  status: TaskStatus;
}

interface TaskPayload {
  id: string;
  task_type: string;
  message: string;
  progress: number | null;
  status: string;
}

// ── Reactive state (wrapped in object so we mutate .list, never reassign) ───

export const taskStore = $state({ list: [] as AppTask[] });
let _unlistens: UnlistenFn[] = [];

// ── Mutation helpers ────────────────────────────────────────────────────────

export function addTask(task: AppTask): void {
  taskStore.list = [...taskStore.list, task];
}

export function updateTask(id: string, patch: Partial<AppTask>): void {
  taskStore.list = taskStore.list.map((t) => (t.id === id ? { ...t, ...patch } : t));
}

export function removeTask(id: string): void {
  taskStore.list = taskStore.list.filter((t) => t.id !== id);
}

// ── Tauri event wiring (call once at app startup) ───────────────────────────

export async function initTaskListeners(): Promise<void> {
  // Guard against double-init
  if (_unlistens.length > 0) return;

  const u1 = await listen<TaskPayload>('app-task-start', (event) => {
    const p = event.payload;
    console.log('[TaskStore] app-task-start', p.id, p.message);
    addTask({
      id: p.id,
      taskType: p.task_type,
      message: p.message,
      progress: p.progress,
      status: 'running',
    });
  });

  const u2 = await listen<TaskPayload>('app-task-progress', (event) => {
    const p = event.payload;
    updateTask(p.id, {
      message: p.message,
      progress: p.progress,
    });
  });

  const u3 = await listen<TaskPayload>('app-task-end', (event) => {
    const p = event.payload;
    console.log('[TaskStore] app-task-end', p.id, p.status, p.message);
    const status: TaskStatus = p.status === 'error' ? 'error' : 'success';
    updateTask(p.id, {
      message: p.message,
      progress: p.progress,
      status,
    });

    // Auto-remove finished tasks after 5 seconds
    setTimeout(() => removeTask(p.id), 5000);
  });

  _unlistens = [u1, u2, u3];
  console.log('[TaskStore] Event listeners registered');
}

export function destroyTaskListeners(): void {
  for (const fn of _unlistens) fn();
  _unlistens = [];
}
