/**
 * Global toast notification store (Svelte 5 runes).
 *
 * Usage:
 *   import { addToast } from '$lib/stores/toast.svelte';
 *   addToast('Saved successfully', 'success');
 *   addToast('Something broke', 'error', 'Oops');
 */

export type ToastType = 'success' | 'error' | 'info' | 'warning';

export interface Toast {
  id: number;
  type: ToastType;
  title: string;
  message: string;
  dismissing: boolean;
}

const DURATIONS: Record<ToastType, number> = {
  success: 4000,
  error: 7000,
  info: 3500,
  warning: 5000,
};

let _nextId = 0;

export const toastStore = $state({ list: [] as Toast[] });

/**
 * Show a toast notification.
 * @param message - Body text
 * @param type - 'success' | 'error' | 'info' | 'warning'
 * @param title - Optional bold title line (defaults to capitalized type)
 * @param duration - Override auto-dismiss duration (ms). 0 = persist.
 */
export function addToast(
  message: string,
  type: ToastType = 'info',
  title?: string,
  duration?: number,
): void {
  const id = ++_nextId;
  const ms = duration ?? DURATIONS[type];
  toastStore.list.push({
    id,
    type,
    title: title ?? type.charAt(0).toUpperCase() + type.slice(1),
    message,
    dismissing: false,
  });

  if (ms > 0) {
    setTimeout(() => dismissToast(id), ms);
  }
}

export function dismissToast(id: number): void {
  const t = toastStore.list.find((t) => t.id === id);
  if (!t) return;
  t.dismissing = true;
  // Remove after CSS animation
  setTimeout(() => {
    toastStore.list = toastStore.list.filter((t) => t.id !== id);
  }, 300);
}

