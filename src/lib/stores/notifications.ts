/**
 * # Notification Store
 *
 * Manages in-app toast notifications and Windows native notifications.
 *
 * - In-app toasts: always shown via Tauri event → Toast.svelte
 * - Windows native: shown only when the app window is not focused
 *
 * ## Usage
 *
 * ```typescript
 * import { notify } from '$lib/stores/notifications';
 *
 * notify.success('Import Complete', '105 files extracted');
 * notify.error('Import Failed', 'Archive is corrupted');
 * notify.importComplete('MyProduct.zip', 105, 130_000_000);
 * notify.batchComplete(10, 2);
 * ```
 */

import { emit } from '@tauri-apps/api/event';
import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from '@tauri-apps/plugin-notification';
import { formatFileSize } from '$lib/api/commands';

type ToastType = 'success' | 'error' | 'info' | 'warning';

let permissionChecked = false;
let hasPermission = false;

/** Check and request notification permission (once). */
async function ensurePermission(): Promise<boolean> {
  if (permissionChecked) return hasPermission;
  try {
    hasPermission = await isPermissionGranted();
    if (!hasPermission) {
      const result = await requestPermission();
      hasPermission = result === 'granted';
    }
  } catch {
    hasPermission = false;
  }
  permissionChecked = true;
  return hasPermission;
}

/** Check if the app window is currently focused. */
function isWindowFocused(): boolean {
  return document.hasFocus();
}

/**
 * Send a notification (in-app toast + native if unfocused).
 */
async function send(type: ToastType, title: string, message: string, duration?: number) {
  // Always emit in-app toast
  await emit('app://notification', { type, title, message, duration });

  // Native notification only when app is in background
  if (!isWindowFocused()) {
    const permitted = await ensurePermission();
    if (permitted) {
      try {
        sendNotification({ title, body: message });
      } catch (err) {
        console.warn('[Notifications] Native notification failed:', err);
      }
    }
  }
}

export const notify = {
  /** Generic notifications */
  success: (title: string, message: string) => send('success', title, message),
  error: (title: string, message: string) => send('error', title, message),
  warning: (title: string, message: string) => send('warning', title, message),
  info: (title: string, message: string) => send('info', title, message),

  /** Import completed successfully */
  importComplete: (name: string, fileCount: number, totalSize: number) =>
    send(
      'success',
      'Import Complete',
      `${name} — ${fileCount} files (${formatFileSize(totalSize)})`
    ),

  /** Import failed */
  importFailed: (name: string, errorMessage: string) =>
    send(
      'error',
      'Import Failed',
      `${name} — ${errorMessage}`
    ),

  /** Batch import completed */
  batchComplete: (succeeded: number, failed: number) => {
    if (failed === 0) {
      return send(
        'success',
        'Batch Complete',
        `All ${succeeded} imports completed successfully`
      );
    }
    return send(
      'warning',
      'Batch Complete',
      `${succeeded} succeeded, ${failed} failed`
    );
  },
};
