<script lang="ts">
  import { onMount } from 'svelte';
  import { t } from '$lib/i18n';
  import { ping } from '$lib/api/commands';

  let pingResult: string = $state('');
  let loading: boolean = $state(true);
  let error: string | null = $state(null);
  let connected: boolean = $state(false);

  onMount(async () => {
    try {
      pingResult = await ping();
      connected = true;
      loading = false;
    } catch (e) {
      error = String(e);
      loading = false;
    }
  });

  async function handlePing() {
    try {
      pingResult = await ping();
      connected = true;
      error = null;
    } catch (e) {
      error = String(e);
      connected = false;
    }
  }
</script>

<div class="status-card">
  {#if loading}
    <div class="loading">{$t('status.connecting')}</div>
  {:else if error}
    <div class="error">{$t('common.error')}: {error}</div>
  {:else}
    <div class="info">
      <h3>FileManagerDaz</h3>
      <p class="version">v0.1.0</p>
      <p class="status" class:connected>{connected ? $t('status.connected') : $t('status.disconnected')}</p>
    </div>
    <div class="ping-section">
      <button onclick={handlePing}>{$t('status.pingBackend')}</button>
      <span class="ping-result">{pingResult}</span>
    </div>
  {/if}
</div>

<style>
  .status-card {
    background-color: var(--bg-secondary);
    border-radius: var(--border-radius);
    padding: 1.5rem;
    text-align: center;
  }

  .info h3 {
    font-size: 1.5rem;
    margin-bottom: 0.5rem;
  }

  .version {
    color: var(--text-secondary);
    font-size: 0.9rem;
  }

  .status {
    color: var(--text-secondary);
    font-weight: bold;
    margin-top: 0.5rem;
  }

  .status.connected {
    color: var(--success);
  }

  .ping-section {
    margin-top: 1.5rem;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 1rem;
  }

  .ping-result {
    color: var(--accent);
    font-family: monospace;
  }

  .loading {
    color: var(--text-secondary);
  }

  .error {
    color: var(--error);
  }
</style>
