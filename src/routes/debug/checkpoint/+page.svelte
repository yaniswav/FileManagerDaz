<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { onMount, onDestroy } from 'svelte';
  import { 
    cleanupTempExtractions, 
    getCheckpointStatus, 
    processBatchRobust,
    type CheckpointData,
    type BatchProgress,
    type BatchOperationResult
  } from '$lib/api/checkpoint';

  // State
  let checkpoint: CheckpointData | null = $state(null);
  let sessionId = $state('');
  let cleanupCount = $state<number | null>(null);
  let isProcessing = $state(false);
  let batchResult: BatchOperationResult | null = $state(null);
  let progress: BatchProgress | null = $state(null);

  // Config
  let testPaths = $state<string[]>([]);
  let unlistenProgress: (() => void) | null = null;

  onMount(() => {
    // Auto-check for latest checkpoint (could add find_latest API)
    sessionId = 'batch_' + Date.now();
  });

  onDestroy(() => {
    if (unlistenProgress) {
      unlistenProgress();
    }
  });

  async function loadCheckpoint() {
    if (!sessionId) return;
    try {
      checkpoint = await getCheckpointStatus(sessionId);
    } catch (error) {
      console.error('Failed to load checkpoint:', error);
    }
  }

  async function cleanup() {
    try {
      cleanupCount = await cleanupTempExtractions();
    } catch (error) {
      console.error('Failed to cleanup:', error);
    }
  }

  async function startBatch() {
    if (testPaths.length === 0) {
      alert('Add some paths first');
      return;
    }

    isProcessing = true;
    progress = null;
    batchResult = null;

    // Listen for progress
    if (unlistenProgress) unlistenProgress();
    unlistenProgress = await listen(`batch-progress-${sessionId}`, (event) => {
      progress = event.payload as BatchProgress;
    });

    try {
      batchResult = await processBatchRobust(testPaths, sessionId);
    } catch (error) {
      console.error('Batch failed:', error);
    } finally {
      isProcessing = false;
      if (unlistenProgress) {
        unlistenProgress();
        unlistenProgress = null;
      }
    }
  }

  function addPath() {
    const path = prompt('Enter path to archive or folder:');
    if (path) {
      testPaths = [...testPaths, path];
    }
  }
</script>

<div class="checkpoint-debug">
  <h1>Checkpoint & Recovery Debug</h1>

  <!-- Checkpoint Status -->
  <section class="card">
    <h2>📋 Checkpoint Status</h2>
    <div class="form-group">
      <label>
        Session ID:
        <input type="text" bind:value={sessionId} placeholder="batch_1234567890" />
      </label>
      <button onclick={loadCheckpoint}>Load Checkpoint</button>
    </div>

    {#if checkpoint}
      <div class="checkpoint-info">
        <p><strong>Session:</strong> {checkpoint.session_id}</p>
        <p><strong>Total Items:</strong> {checkpoint.total_items}</p>
        <p><strong>Processed:</strong> {checkpoint.processed.length} ({(checkpoint.processed.length / checkpoint.total_items * 100).toFixed(1)}%)</p>
        <p><strong>Failed:</strong> {checkpoint.failed.length}</p>
        <p><strong>Last Update:</strong> {new Date(checkpoint.last_update * 1000).toLocaleString()}</p>

        {#if checkpoint.failed.length > 0}
          <details>
            <summary>Failed Items ({checkpoint.failed.length})</summary>
            <ul class="failed-list">
              {#each checkpoint.failed as item}
                <li>
                  <strong>{item.path}</strong><br />
                  <small>{item.error}</small>
                </li>
              {/each}
            </ul>
          </details>
        {/if}
      </div>
    {:else}
      <p class="empty">No checkpoint found or not loaded yet.</p>
    {/if}
  </section>

  <!-- Cleanup -->
  <section class="card">
    <h2>🧹 Temp Directory Cleanup</h2>
    <p>Remove all <code>*_extracted</code> folders from temp directory.</p>
    <button onclick={cleanup}>Run Cleanup</button>

    {#if cleanupCount !== null}
      <p class="result success">✅ Cleaned {cleanupCount} folders</p>
    {/if}
  </section>

  <!-- Test Batch -->
  <section class="card">
    <h2>🚀 Test Batch Processing</h2>
    
    <div class="path-list">
      <h3>Test Paths ({testPaths.length})</h3>
      {#each testPaths as path, i}
        <div class="path-item">
          <span>{path}</span>
          <button onclick={() => testPaths = testPaths.filter((_, j) => j !== i)}>Remove</button>
        </div>
      {/each}
      <button onclick={addPath}>+ Add Path</button>
    </div>

    <button onclick={startBatch} disabled={isProcessing || testPaths.length === 0}>
      {isProcessing ? 'Processing...' : 'Start Batch'}
    </button>

    {#if progress}
      <div class="progress-info">
        <h3>Progress</h3>
        <progress value={progress.completed} max={progress.total}></progress>
        <p>{progress.completed} / {progress.total} ({(progress.completed / progress.total * 100).toFixed(1)}%)</p>
        <p>✅ Succeeded: {progress.succeeded} | ❌ Failed: {progress.failed}</p>
        {#if progress.current_item}
          <p class="current-item"><small>Current: {progress.current_item}</small></p>
        {/if}
      </div>
    {/if}

    {#if batchResult}
      <div class="result-info">
        <h3>Results</h3>
        <p><strong>Total:</strong> {batchResult.stats.total_items}</p>
        <p class="success"><strong>Successful:</strong> {batchResult.stats.successful}</p>
        <p class="error"><strong>Failed:</strong> {batchResult.stats.failed}</p>
        <p><strong>Total Files:</strong> {batchResult.stats.total_files}</p>
        <p><strong>Total Size:</strong> {(batchResult.stats.total_size_bytes / 1024 / 1024).toFixed(2)} MB</p>
        <p><strong>Duration:</strong> {batchResult.stats.duration_seconds}s</p>

        {#if batchResult.failures.length > 0}
          <details>
            <summary>Failures ({batchResult.failures.length})</summary>
            <ul class="failed-list">
              {#each batchResult.failures as failure}
                <li>
                  <strong>{failure.source_path}</strong><br />
                  <small>[{failure.error_code}] {failure.error}</small>
                </li>
              {/each}
            </ul>
          </details>
        {/if}
      </div>
    {/if}
  </section>

  <!-- Documentation -->
  <section class="card">
    <h2>📚 Documentation</h2>
    <p>See <code>docs/CHECKPOINT.md</code> for full documentation.</p>
    <ul>
      <li><strong>Checkpoint Location:</strong> <code>%TEMP%\FileManagerDaz\checkpoints\</code></li>
      <li><strong>Temp Extractions:</strong> <code>%TEMP%\FileManagerDaz\downloads_import\</code></li>
      <li><strong>Auto-Resume:</strong> Use same session ID to resume interrupted batch</li>
    </ul>
  </section>
</div>

<style>
  .checkpoint-debug {
    padding: 2rem;
    max-width: 1200px;
    margin: 0 auto;
  }

  h1 {
    margin-bottom: 2rem;
  }

  .card {
    background: var(--color-bg-elevated);
    border: 1px solid var(--color-border);
    border-radius: 8px;
    padding: 1.5rem;
    margin-bottom: 1.5rem;
  }

  .card h2 {
    margin-top: 0;
    margin-bottom: 1rem;
  }

  .form-group {
    display: flex;
    gap: 1rem;
    margin-bottom: 1rem;
  }

  .form-group label {
    flex: 1;
  }

  input[type="text"] {
    width: 100%;
    padding: 0.5rem;
    border: 1px solid var(--color-border);
    border-radius: 4px;
    font-family: 'Consolas', monospace;
  }

  button {
    padding: 0.5rem 1rem;
    background: var(--color-primary);
    color: white;
    border: none;
    border-radius: 4px;
    cursor: pointer;
  }

  button:hover {
    opacity: 0.9;
  }

  button:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .checkpoint-info {
    background: var(--color-bg);
    padding: 1rem;
    border-radius: 4px;
  }

  .checkpoint-info p {
    margin: 0.5rem 0;
  }

  .empty {
    color: var(--color-text-secondary);
    font-style: italic;
  }

  .result {
    margin-top: 1rem;
    padding: 1rem;
    border-radius: 4px;
  }

  .result.success {
    background: #d4edda;
    color: #155724;
  }

  .result.error {
    background: #f8d7da;
    color: #721c24;
  }

  .path-list {
    margin-bottom: 1rem;
  }

  .path-item {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 0.5rem;
    background: var(--color-bg);
    border-radius: 4px;
    margin-bottom: 0.5rem;
  }

  .path-item span {
    font-family: 'Consolas', monospace;
    font-size: 0.9rem;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .progress-info, .result-info {
    margin-top: 1rem;
    padding: 1rem;
    background: var(--color-bg);
    border-radius: 4px;
  }

  progress {
    width: 100%;
    height: 24px;
    margin-bottom: 0.5rem;
  }

  .current-item {
    color: var(--color-text-secondary);
    word-break: break-all;
  }

  .success {
    color: #28a745;
  }

  .error {
    color: #dc3545;
  }

  .failed-list {
    margin-top: 0.5rem;
    list-style: none;
    padding: 0;
  }

  .failed-list li {
    padding: 0.5rem;
    background: var(--color-bg-elevated);
    border-left: 3px solid #dc3545;
    margin-bottom: 0.5rem;
  }

  details {
    margin-top: 1rem;
  }

  summary {
    cursor: pointer;
    font-weight: bold;
    padding: 0.5rem;
    background: var(--color-bg-elevated);
    border-radius: 4px;
  }

  summary:hover {
    background: var(--color-bg);
  }

  code {
    background: var(--color-bg);
    padding: 0.2rem 0.4rem;
    border-radius: 3px;
    font-family: 'Consolas', monospace;
    font-size: 0.9em;
  }
</style>
