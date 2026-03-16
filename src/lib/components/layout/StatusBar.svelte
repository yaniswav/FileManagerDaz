<script lang="ts">
  import { taskStore } from '$lib/stores/tasks.svelte';
  import { toggleLogPanel, logStore } from '$lib/stores/tasklog.svelte';

  let activeTask = $derived.by(() => {
    const running = taskStore.list.filter((t) => t.status === 'running');
    return running.length > 0 ? running[running.length - 1] : null;
  });

  let allRunningCount = $derived(taskStore.list.filter((t) => t.status === 'running').length);
  let logCount = $derived(logStore.entries.length);

  // Show finished tasks briefly (success/error) when no running task
  let displayTask = $derived(activeTask ?? taskStore.list[taskStore.list.length - 1] ?? null);

  let progressPercent = $derived(
    displayTask?.progress != null ? Math.round(displayTask.progress * 100) : null,
  );
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<footer class="status-bar" onclick={toggleLogPanel}>
  {#if displayTask}
    <div class="status-left">
      {#if displayTask.status === 'running'}
        <svg class="spinner" viewBox="0 0 16 16" fill="none" xmlns="http://www.w3.org/2000/svg">
          <circle cx="8" cy="8" r="6" stroke="currentColor" stroke-width="2" stroke-dasharray="28 10" />
        </svg>
      {:else if displayTask.status === 'success'}
        <span class="icon-done">✓</span>
      {:else}
        <span class="icon-error">✕</span>
      {/if}
      <span class="status-message">{displayTask.message}</span>
    </div>

    <div class="status-right">
      {#if progressPercent != null && displayTask.status === 'running'}
        <span class="progress-text">{progressPercent}%</span>
        <div class="progress-bar-track">
          <div class="progress-bar-fill" style="width: {progressPercent}%"></div>
        </div>
      {/if}
      {#if allRunningCount > 1}
        <span class="task-count">{allRunningCount} tasks</span>
      {/if}
    </div>
  {:else}
    <div class="status-left">
      <span class="status-ready">Ready</span>
    </div>
    {#if logCount > 0}
      <div class="status-right">
        <span class="log-indicator">{logCount} log{logCount > 1 ? 's' : ''}</span>
      </div>
    {/if}
  {/if}
</footer>

<style>
  .status-bar {
    position: fixed;
    bottom: 0;
    left: 0;
    right: 0;
    height: 28px;
    z-index: 9997;
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 12px;
    background-color: var(--bg-secondary, #16213e);
    border-top: 1px solid var(--border-color, #333);
    font-size: 0.75rem;
    color: var(--text-secondary, #a0a0a0);
    user-select: none;
    cursor: pointer;
  }

  .status-left {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
    overflow: hidden;
  }

  .status-right {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-shrink: 0;
  }

  .status-message {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    color: var(--text-primary, #eaeaea);
  }

  .status-ready {
    color: var(--text-secondary, #a0a0a0);
    font-style: italic;
  }

  /* Spinner animation */
  .spinner {
    width: 14px;
    height: 14px;
    flex-shrink: 0;
    color: var(--accent, #e94560);
    animation: spin 1s linear infinite;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  .icon-done {
    color: var(--success, #4ecca3);
    font-weight: bold;
    font-size: 0.85rem;
  }

  .icon-error {
    color: var(--error, #e94560);
    font-weight: bold;
    font-size: 0.85rem;
  }

  /* Progress bar */
  .progress-text {
    font-variant-numeric: tabular-nums;
    min-width: 32px;
    text-align: right;
  }

  .progress-bar-track {
    width: 80px;
    height: 4px;
    border-radius: 2px;
    background: rgba(255, 255, 255, 0.1);
    overflow: hidden;
  }

  .progress-bar-fill {
    height: 100%;
    border-radius: 2px;
    background: var(--accent, #e94560);
    transition: width 0.3s ease;
  }

  .task-count {
    color: var(--text-secondary, #a0a0a0);
    border-left: 1px solid var(--border-color, #333);
    padding-left: 8px;
  }

  .log-indicator {
    color: var(--text-muted, #5c5c72);
    font-size: 0.7rem;
  }
</style>
