<script lang="ts">
  import { logStore, logPanelOpen, clearLog } from '$lib/stores/tasklog.svelte';
  import { onMount, tick } from 'svelte';

  let scrollEl: HTMLDivElement;
  let autoScroll = $state(true);

  const entries = $derived(logStore.entries);
  const isOpen = $derived(logPanelOpen.value);

  function formatTime(ts: number): string {
    const d = new Date(ts);
    return d.toLocaleTimeString('en-GB', { hour: '2-digit', minute: '2-digit', second: '2-digit' });
  }

  function levelIcon(level: string): string {
    switch (level) {
      case 'success': return '✓';
      case 'error': return '✕';
      case 'warning': return '!';
      case 'running': return '›';
      default: return '·';
    }
  }

  // Auto-scroll when new entries arrive
  $effect(() => {
    if (entries.length && autoScroll && scrollEl) {
      tick().then(() => {
        scrollEl.scrollTop = scrollEl.scrollHeight;
      });
    }
  });

  function handleScroll() {
    if (!scrollEl) return;
    const atBottom = scrollEl.scrollHeight - scrollEl.scrollTop - scrollEl.clientHeight < 40;
    autoScroll = atBottom;
  }
</script>

{#if isOpen}
  <div class="task-logger">
    <div class="logger-header">
      <span class="logger-title">Activity Log</span>
      <div class="logger-actions">
        <button class="logger-btn" onclick={clearLog} title="Clear log">Clear</button>
        <button class="logger-btn" onclick={() => logPanelOpen.value = false} title="Close">✕</button>
      </div>
    </div>
    <div class="logger-body" bind:this={scrollEl} onscroll={handleScroll}>
      {#if entries.length === 0}
        <div class="logger-empty">No activity yet</div>
      {:else}
        {#each entries as entry (entry.id)}
          <div class="log-line" class:log-success={entry.level === 'success'} class:log-error={entry.level === 'error'} class:log-warning={entry.level === 'warning'} class:log-running={entry.level === 'running'}>
            <span class="log-time">{formatTime(entry.timestamp)}</span>
            <span class="log-badge badge-{entry.level}">{levelIcon(entry.level)}</span>
            <span class="log-msg">{entry.message}</span>
          </div>
        {/each}
      {/if}
    </div>
  </div>
{/if}

<style>
  .task-logger {
    position: fixed;
    bottom: 28px;
    left: 0;
    right: 0;
    height: 220px;
    z-index: 9995;
    display: flex;
    flex-direction: column;
    background: var(--bg-primary, #0f0f14);
    border-top: 1px solid var(--surface-border, rgba(255,255,255,0.08));
    animation: slide-up 0.2s var(--ease-out, cubic-bezier(0.2,0.8,0.2,1));
  }

  @keyframes slide-up {
    from { transform: translateY(100%); opacity: 0; }
    to   { transform: translateY(0); opacity: 1; }
  }

  .logger-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 12px;
    height: 32px;
    flex-shrink: 0;
    background: var(--bg-secondary, #16161e);
    border-bottom: 1px solid var(--surface-border, rgba(255,255,255,0.06));
  }

  .logger-title {
    font-size: 0.75rem;
    font-weight: 600;
    color: var(--text-secondary, #8b8b9e);
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  .logger-actions {
    display: flex;
    gap: 4px;
  }

  .logger-btn {
    padding: 2px 8px;
    font-size: 0.7rem;
    background: transparent;
    color: var(--text-muted, #5c5c72);
    border: 1px solid var(--surface-border, rgba(255,255,255,0.08));
    border-radius: 4px;
    cursor: pointer;
    transition: all 0.15s;
    transform: none;
    box-shadow: none;
  }

  .logger-btn:hover {
    color: var(--text-primary, #ececf1);
    background: var(--surface-elevated, rgba(255,255,255,0.05));
    transform: none;
    box-shadow: none;
  }

  .logger-body {
    flex: 1;
    overflow-y: auto;
    padding: 4px 0;
    font-family: 'SF Mono', 'Cascadia Code', 'Fira Code', monospace;
    font-size: 0.72rem;
    line-height: 1.7;
  }

  .logger-empty {
    padding: 2rem;
    text-align: center;
    color: var(--text-muted, #5c5c72);
    font-family: inherit;
    font-style: italic;
  }

  .log-line {
    display: flex;
    align-items: baseline;
    gap: 8px;
    padding: 1px 12px;
    color: var(--text-secondary, #8b8b9e);
    transition: background 0.1s;
  }

  .log-line:hover {
    background: rgba(255, 255, 255, 0.02);
  }

  .log-time {
    color: var(--text-muted, #5c5c72);
    flex-shrink: 0;
    font-variant-numeric: tabular-nums;
  }

  .log-badge {
    flex-shrink: 0;
    width: 14px;
    text-align: center;
    font-weight: 700;
  }

  .badge-info    { color: var(--text-muted, #5c5c72); }
  .badge-running { color: var(--accent, #e94560); }
  .badge-success { color: var(--success, #4ecca3); }
  .badge-error   { color: var(--error, #e94560); }
  .badge-warning { color: var(--warning, #ffc107); }

  .log-msg {
    min-width: 0;
    word-break: break-word;
  }

  .log-success .log-msg { color: var(--success, #4ecca3); }
  .log-error   .log-msg { color: #fca5a5; }
  .log-warning .log-msg { color: var(--warning, #ffc107); }
  .log-running .log-msg { color: var(--text-primary, #ececf1); }
</style>
