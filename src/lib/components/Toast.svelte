<!--
  Toast Notification Component
  
  Displays in-app toast notifications (success, error, info, warning).
  Reads from the global toastStore. Auto-dismisses via the store.
  Also listens for backend 'app://notification' events.
-->
<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { listen, type UnlistenFn } from '@tauri-apps/api/event';
  import { toastStore, addToast, dismissToast, type Toast, type ToastType } from '$lib/stores/toast.svelte';

  interface NotificationEvent {
    type: ToastType;
    title: string;
    message: string;
    duration?: number;
  }

  let unlistenNotification: UnlistenFn | null = null;

  let toasts: Toast[] = $derived(toastStore.list);

  function getIcon(type: ToastType): string {
    switch (type) {
      case 'success': return '✅';
      case 'error': return '❌';
      case 'warning': return '⚠️';
      case 'info': return 'ℹ️';
    }
  }

  onMount(async () => {
    unlistenNotification = await listen<NotificationEvent>('app://notification', (event) => {
      const { type, title, message, duration } = event.payload;
      addToast(message, type, title, duration);
    });
  });

  onDestroy(() => {
    unlistenNotification?.();
  });
</script>

{#if toasts.length > 0}
  <div class="toast-container">
    {#each toasts as toast (toast.id)}
      <div
        class="toast toast-{toast.type}"
        class:dismissing={toast.dismissing}
        role="alert"
      >
        <span class="toast-icon">{getIcon(toast.type)}</span>
        <div class="toast-content">
          <strong class="toast-title">{toast.title}</strong>
          <p class="toast-message">{toast.message}</p>
        </div>
        <button
          type="button"
          class="toast-close"
          onclick={() => dismissToast(toast.id)}
          aria-label="Close"
        >✕</button>
      </div>
    {/each}
  </div>
{/if}

<style>
  .toast-container {
    position: fixed;
    bottom: 1.5rem;
    right: 1.5rem;
    display: flex;
    flex-direction: column-reverse;
    gap: 0.5rem;
    z-index: 9998;
    pointer-events: none;
    max-width: 400px;
  }

  .toast {
    display: flex;
    align-items: flex-start;
    gap: 0.75rem;
    padding: 0.85rem 1rem;
    border-radius: 10px;
    background: var(--bg-tertiary, #252542);
    border-left: 4px solid;
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.4);
    pointer-events: auto;
    animation: slideIn 0.3s ease-out;
    transition: opacity 0.3s, transform 0.3s;
  }

  .toast.dismissing {
    opacity: 0;
    transform: translateX(100%);
  }

  .toast-success {
    border-left-color: var(--success, #4ade80);
  }

  .toast-error {
    border-left-color: var(--error, #f87171);
  }

  .toast-warning {
    border-left-color: var(--warning, #fbbf24);
  }

  .toast-info {
    border-left-color: var(--accent, #646cff);
  }

  .toast-icon {
    font-size: 1.1rem;
    flex-shrink: 0;
    margin-top: 0.1rem;
  }

  .toast-content {
    flex: 1;
    min-width: 0;
  }

  .toast-title {
    display: block;
    font-size: 0.85rem;
    color: var(--text-primary, #fff);
    margin-bottom: 0.15rem;
  }

  .toast-message {
    font-size: 0.78rem;
    color: var(--text-secondary, #888);
    margin: 0;
    line-height: 1.4;
    word-break: break-word;
  }

  .toast-close {
    background: none;
    border: none;
    color: var(--text-secondary, #888);
    cursor: pointer;
    font-size: 0.8rem;
    padding: 0.15rem;
    opacity: 0.5;
    transition: opacity 0.15s;
    flex-shrink: 0;
  }

  .toast-close:hover {
    opacity: 1;
  }

  @keyframes slideIn {
    from {
      opacity: 0;
      transform: translateX(100%);
    }
    to {
      opacity: 1;
      transform: translateX(0);
    }
  }
</style>
