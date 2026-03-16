/**
 * # Task Log Store
 *
 * Persistent log of all background activity (watcher events, extractions,
 * scans, downloads). Powers the TaskLogger panel.
 *
 * Subscribes to the same Tauri events as tasks.svelte.ts but keeps a
 * rolling history instead of auto-removing entries.
 */

import { listen, type UnlistenFn } from '@tauri-apps/api/event';

export type LogLevel = 'info' | 'success' | 'error' | 'warning' | 'running';

export interface LogEntry {
  id: string;
  timestamp: number;
  level: LogLevel;
  message: string;
  detail?: string;
}

const MAX_LOG_ENTRIES = 200;

export const logStore = $state({ entries: [] as LogEntry[] });
export const logPanelOpen = $state({ value: false });

let _unlistens: UnlistenFn[] = [];
let _counter = 0;

function uid(): string {
  return `log-${Date.now()}-${++_counter}`;
}

export function pushLog(level: LogLevel, message: string, detail?: string): void {
  const entry: LogEntry = { id: uid(), timestamp: Date.now(), level, message, detail };
  logStore.entries = [...logStore.entries.slice(-(MAX_LOG_ENTRIES - 1)), entry];
}

export function clearLog(): void {
  logStore.entries = [];
}

export function toggleLogPanel(): void {
  logPanelOpen.value = !logPanelOpen.value;
}

// ── Tauri event wiring ──────────────────────────────────────────────────────

interface TaskPayload {
  id: string;
  task_type: string;
  message: string;
  progress: number | null;
  status: string;
}

export async function initLogListeners(): Promise<void> {
  if (_unlistens.length > 0) return;

  const u1 = await listen<TaskPayload>('app-task-start', (e) => {
    const p = e.payload;
    pushLog('running', p.message, `Task ${p.task_type} started`);
  });

  const u2 = await listen<TaskPayload>('app-task-progress', (e) => {
    const p = e.payload;
    if (p.message) {
      pushLog('info', p.message);
    }
  });

  const u3 = await listen<TaskPayload>('app-task-end', (e) => {
    const p = e.payload;
    const level: LogLevel = p.status === 'error' ? 'error' : 'success';
    pushLog(level, p.message, `Task ${p.task_type} finished`);
  });

  // Watcher-specific events (file detected)
  const u4 = await listen<{ path: string; event_type: string }>('watcher-file-ready', (e) => {
    const name = e.payload.path.split(/[\\/]/).pop() || 'file';
    pushLog('info', `Archive detected: ${name}`, e.payload.path);
  });

  _unlistens = [u1, u2, u3, u4];
}

export function destroyLogListeners(): void {
  for (const fn of _unlistens) fn();
  _unlistens = [];
}
