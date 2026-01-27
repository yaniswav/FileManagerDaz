<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { listen, type UnlistenFn } from '@tauri-apps/api/event';
  import { open } from '@tauri-apps/plugin-dialog';
  import { formatFileSize, type RecursiveExtractResult } from '$lib/api/commands';
  import { t } from '$lib/i18n';
  import {
    importsStore,
    isProcessing as isProcessingStore,
    retryableTasks as retryableTasksStore,
    processMultipleSources,
    retryTask,
    retryAllFailed,
    initStepListener,
    type ImportTask,
    type ImportStatus,
  } from '$lib/stores/imports';

  // Props using Svelte 5 runes
  interface Props {
    disabled?: boolean;
    onprocessed?: (result: RecursiveExtractResult) => void;
    onerror?: (data: { path: string; message: string }) => void;
  }
  
  let { disabled = false, onprocessed, onerror }: Props = $props();

  // Local state (UI-only for drag styling)
  let isDragOver = $state(false);
  
  // Store state (reactive)
  let tasks: ImportTask[] = $state([]);
  let isProcessing = $state(false);
  let retryableCount = $state(0);
  
  // Expanded states for each task (local as UI only)
  let expandedTasks: Set<string> = $state(new Set());
  
  // Store subscriptions
  const unsubscribeTasks = importsStore.subscribe(value => {
    tasks = value;
  });
  
  const unsubscribeProcessing = isProcessingStore.subscribe(value => {
    isProcessing = value;
  });

  const unsubscribeRetryable = retryableTasksStore.subscribe(value => {
    retryableCount = value.length;
  });
  
  // Computed
  let hasResults = $derived(tasks.length > 0);

  // Tauri drag-drop listeners
  let unlistenDragEnter: UnlistenFn | null = null;
  let unlistenDragLeave: UnlistenFn | null = null;
  let unlistenDrop: UnlistenFn | null = null;

  onMount(async () => {
    console.log('[DropZone] onMount - Setting up listeners...');
    
    // Load tasks from database
    await importsStore.loadFromDatabase();
    
    // Initialize step listener
    await initStepListener();
    
    try {
      unlistenDragEnter = await listen('tauri://drag-enter', () => {
        if (!disabled) isDragOver = true;
      });

      unlistenDragLeave = await listen('tauri://drag-leave', () => {
        isDragOver = false;
      });

      unlistenDrop = await listen<{ paths: string[] }>('tauri://drag-drop', async (event) => {
        console.log('[DropZone] tauri://drag-drop received:', event);
        isDragOver = false;
        if (disabled) return;
        
        const paths = event.payload.paths;
        if (paths && paths.length > 0) {
          await handleProcessSources(paths);
        }
      });
      console.log('[DropZone] All Tauri listeners ready!');
    } catch (e) {
      console.error('[DropZone] Failed to setup Tauri listeners:', e);
    }
  });

  onDestroy(() => {
    console.log('[DropZone] onDestroy - Cleaning up');
    unlistenDragEnter?.();
    unlistenDragLeave?.();
    unlistenDrop?.();
    unsubscribeTasks();
    unsubscribeProcessing();
    unsubscribeRetryable();
  });

  // Lance le traitement via le store global
  async function handleProcessSources(paths: string[]) {
    await processMultipleSources(paths, undefined, onprocessed, onerror);
  }

  // Retry all failed or interrupted tasks
  async function handleRetryAll() {
    await retryAllFailed(onprocessed, onerror);
  }

  // Open native file picker
  async function openFilePicker() {
    if (disabled || isProcessing) return;

    try {
      const selected = await open({
        multiple: true,
        directory: false,
        filters: [
          { name: 'Archives', extensions: ['zip', 'rar', '7z'] },
          { name: 'All files', extensions: ['*'] }
        ],
        title: $t('import.selectFiles')
      });

      if (selected) {
        const paths = Array.isArray(selected) ? selected : [selected];
        if (paths.length > 0) {
          await handleProcessSources(paths);
        }
      }
    } catch (e) {
      console.error('[DropZone] File picker error:', e);
    }
  }

  // Open folder picker
  async function openFolderPicker() {
    if (disabled || isProcessing) return;

    try {
      const selected = await open({
        multiple: true,
        directory: true,
        title: $t('import.selectFolders')
      });

      if (selected) {
        const paths = Array.isArray(selected) ? selected : [selected];
        if (paths.length > 0) {
          await handleProcessSources(paths);
        }
      }
    } catch (e) {
      console.error('[DropZone] Folder picker error:', e);
    }
  }

  async function clearResults() {
    await importsStore.clearCompleted();
    expandedTasks.clear();
  }

  function removeItem(id: string) {
    importsStore.removeTask(id);
    expandedTasks.delete(id);
  }

  async function handleRetry(task: ImportTask) {
    await retryTask(task.id, onprocessed, onerror);
  }

  function toggleExpanded(id: string) {
    if (expandedTasks.has(id)) {
      expandedTasks.delete(id);
    } else {
      expandedTasks.add(id);
    }
    // Force reactivity
    expandedTasks = new Set(expandedTasks);
  }

  // Native HTML drag handling (for hover styling)
  function handleDragOver(e: DragEvent) {
    e.preventDefault();
    e.stopPropagation();
    if (!disabled) isDragOver = true;
  }

  function handleDragLeave(e: DragEvent) {
    e.preventDefault();
    e.stopPropagation();
    isDragOver = false;
  }

  function handleDrop(e: DragEvent) {
    e.preventDefault();
    e.stopPropagation();
    isDragOver = false;
  }

  function getStatusIcon(status: ImportStatus): string {
    switch (status) {
      case 'pending': return '⏳';
      case 'processing': return '🔄';
      case 'done': return '✅';
      case 'error': return '❌';
      case 'interrupted': return '⚠️';
      default: return '❓';
    }
  }

  function getFormatIcon(format: string | null | undefined): string {
    switch (format) {
      case 'zip': return '📦';
      case 'rar': return '📦';
      case 'sevenzip': return '📦';
      default: return '📁';
    }
  }

  // Formats destination path for display
  function formatDestination(dest: string): string {
    if (dest.length > 50) {
      return '...' + dest.slice(-47);
    }
    return dest;
  }
</script>

<div class="drop-zone-container">
  <!-- Zone de drop principale -->
  <button
    type="button"
    class="drop-zone"
    class:drag-over={isDragOver}
    class:processing={isProcessing}
    class:disabled
    ondragover={handleDragOver}
    ondragleave={handleDragLeave}
    ondrop={handleDrop}
    onclick={openFilePicker}
  >
    {#if isProcessing}
      <div class="zone-content">
        <div class="spinner"></div>
        <p class="title">{$t('import.processing')}</p>
        <p class="subtitle">{$t('import.filesInProgress', { count: tasks.filter(i => i.status === 'processing').length.toString() })}</p>
      </div>
    {:else}
      <div class="zone-content">
        <span class="icon">📦</span>
        <p class="title">{$t('import.dropZone.title')}</p>
        <p class="subtitle">{$t('import.dropZone.subtitle')}</p>
        <div class="buttons">
          <span class="picker-btn" role="button" tabindex="0" onclick={(e) => { e.stopPropagation(); openFilePicker(); }} onkeydown={(e) => e.key === 'Enter' && openFilePicker()}>
            📄 {$t('import.dropZone.files')}
          </span>
          <span class="picker-btn" role="button" tabindex="0" onclick={(e) => { e.stopPropagation(); openFolderPicker(); }} onkeydown={(e) => e.key === 'Enter' && openFolderPicker()}>
            📁 {$t('import.dropZone.folders')}
          </span>
        </div>
      </div>
    {/if}
  </button>

  <!-- Results list -->
  {#if hasResults}
    <div class="results-section">
      <div class="results-header">
        <h3>{$t('import.results.title')} ({tasks.length})</h3>
        <div class="header-actions">
          {#if retryableCount > 0 && !isProcessing}
            <button type="button" class="retry-all-btn" onclick={handleRetryAll}>
              🔄 {$t('import.results.retryAll')} ({retryableCount})
            </button>
          {/if}
          {#if !isProcessing}
            <button type="button" class="clear-btn" onclick={clearResults}>
              {$t('import.results.clear')}
            </button>
          {/if}
        </div>
      </div>

      <ul class="results-list">
        {#each tasks as task (task.id)}
          <li class="result-item" class:error={task.status === 'error'} class:done={task.status === 'done'} class:processing={task.status === 'processing'} class:interrupted={task.status === 'interrupted'}>
            <div class="item-main">
              <span class="item-status">{getStatusIcon(task.status)}</span>
              <span class="item-icon">{getFormatIcon(task.result?.archive_format)}</span>
              <div class="item-info">
                <span class="item-name">{task.name}</span>
                
                <!-- Current step during processing -->
                {#if task.status === 'processing' && task.currentStep}
                  <span class="item-current-step">
                    <span class="step-spinner"></span>
                    {task.currentStep}
                  </span>
                {:else if task.status === 'done' && task.result}
                  <span class="item-stats">
                    {task.result.total_files} {$t('common.files')} · {formatFileSize(task.result.total_size)}
                    {#if task.result.archive_format}
                      · {task.result.archive_format.toUpperCase()}
                    {/if}
                    {#if task.result.nested_archives && task.result.nested_archives.length > 0}
                      · {task.result.nested_archives.length} {$t('import.nestedArchives')}
                    {/if}
                  </span>
                  <!-- Chemin d'extraction -->
                  <span class="item-destination" title={task.result.destination}>
                    📂 {formatDestination(task.result.destination)}
                    {#if task.result.moved_to_library}
                      <span class="library-badge">{$t('import.library')}</span>
                    {/if}
                  </span>
                {:else if task.status === 'error'}
                  <span class="item-error">{task.error}</span>
                {:else if task.status === 'interrupted'}
                  <span class="item-interrupted">{$t('import.interrupted')}</span>
                {:else if task.status === 'pending'}
                  <span class="item-pending">{$t('import.pending')}</span>
                {/if}
              </div>
              
              <!-- Actions -->
              <div class="item-actions">
                {#if task.steps.length > 0}
                  <button 
                    type="button" 
                    class="action-btn expand" 
                    onclick={() => toggleExpanded(task.id)} 
                    title={expandedTasks.has(task.id) ? $t('import.hideLog') : $t('import.showLog')}
                  >
                    {expandedTasks.has(task.id) ? '▼' : '▶'}
                  </button>
                {/if}
                {#if task.status === 'error' || task.status === 'interrupted'}
                  <button type="button" class="action-btn retry" onclick={() => handleRetry(task)} title={$t('common.retry')}>
                    🔄
                  </button>
                {/if}
                {#if task.status !== 'processing'}
                  <button type="button" class="action-btn remove" onclick={() => removeItem(task.id)} title={$t('common.delete')}>
                    ✕
                  </button>
                {/if}
              </div>
            </div>

            <!-- Steps log (collapsible) -->
            {#if expandedTasks.has(task.id) && task.steps.length > 0}
              <div class="steps-log">
                <div class="steps-header">{$t('import.stepsLog')}</div>
                <ul class="steps-list">
                  {#each task.steps as step}
                    <li class="step-item">
                      <span class="step-time">[{step.time}]</span>
                      <span class="step-message">{step.message}</span>
                      {#if step.details}
                        <span class="step-details">{step.details}</span>
                      {/if}
                    </li>
                  {/each}
                </ul>
              </div>
            {/if}
          </li>
        {/each}
      </ul>
    </div>
  {/if}
</div>

<style>
  .drop-zone-container {
    display: flex;
    flex-direction: column;
    gap: 1rem;
  }

  .drop-zone {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    min-height: 180px;
    padding: 2rem;
    border: 2px dashed var(--border-color, #444);
    border-radius: 12px;
    background: var(--bg-secondary, #1a1a2e);
    transition: all 0.2s ease;
    cursor: pointer;
    user-select: none;
    width: 100%;
    font-family: inherit;
    color: inherit;
  }

  .drop-zone:hover:not(.disabled):not(.processing) {
    border-color: var(--accent, #646cff);
    background: var(--bg-hover, #1e1e3a);
  }

  .drop-zone.drag-over {
    border-color: var(--accent, #646cff);
    border-style: solid;
    background: rgba(100, 108, 255, 0.15);
    transform: scale(1.01);
    box-shadow: 0 0 30px rgba(100, 108, 255, 0.3);
  }

  .drop-zone.disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .drop-zone.processing {
    cursor: wait;
    border-style: solid;
    border-color: var(--accent, #646cff);
  }

  .zone-content {
    text-align: center;
  }

  .icon {
    font-size: 3rem;
    display: block;
    margin-bottom: 0.75rem;
  }

  .title {
    font-size: 1.1rem;
    font-weight: 600;
    margin: 0 0 0.25rem;
    color: var(--text-primary, #fff);
  }

  .subtitle {
    font-size: 0.85rem;
    color: var(--text-secondary, #888);
    margin: 0 0 1rem;
  }

  .buttons {
    display: flex;
    gap: 0.5rem;
    justify-content: center;
  }

  .picker-btn {
    padding: 0.5rem 1rem;
    background: var(--bg-tertiary, #252542);
    border: 1px solid var(--border-color, #444);
    border-radius: 6px;
    color: var(--text-primary, #fff);
    cursor: pointer;
    font-size: 0.85rem;
    transition: all 0.15s;
  }

  .picker-btn:hover {
    background: var(--accent, #646cff);
    border-color: var(--accent, #646cff);
  }

  .spinner {
    width: 40px;
    height: 40px;
    border: 3px solid var(--border-color, #444);
    border-top-color: var(--accent, #646cff);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
    margin: 0 auto 1rem;
  }

  .step-spinner {
    display: inline-block;
    width: 12px;
    height: 12px;
    border: 2px solid var(--border-color, #444);
    border-top-color: var(--accent, #646cff);
    border-radius: 50%;
    animation: spin 0.6s linear infinite;
    margin-right: 0.5rem;
    vertical-align: middle;
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }

  /* Section résultats */
  .results-section {
    background: var(--bg-secondary, #1a1a2e);
    border-radius: 12px;
    padding: 1rem;
  }

  .results-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 0.75rem;
  }

  .results-header h3 {
    margin: 0;
    font-size: 0.95rem;
    color: var(--text-primary, #fff);
  }

  .header-actions {
    display: flex;
    gap: 0.5rem;
  }

  .retry-all-btn {
    background: var(--accent, #646cff);
    color: white;
    border: none;
    cursor: pointer;
    font-size: 0.8rem;
    padding: 0.35rem 0.75rem;
    border-radius: 4px;
  }

  .retry-all-btn:hover {
    background: var(--accent-hover, #535bf2);
  }

  .clear-btn {
    background: none;
    border: none;
    color: var(--text-secondary, #888);
    cursor: pointer;
    font-size: 0.8rem;
    padding: 0.25rem 0.5rem;
  }

  .clear-btn:hover {
    color: var(--text-primary, #fff);
  }

  .results-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  .result-item {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    padding: 0.75rem;
    background: var(--bg-tertiary, #252542);
    border-radius: 8px;
    border-left: 3px solid var(--border-color, #444);
  }

  .result-item.done {
    border-left-color: var(--success, #4ade80);
  }

  .result-item.error {
    border-left-color: var(--error, #f87171);
  }

  .result-item.processing {
    border-left-color: var(--accent, #646cff);
  }

  .item-main {
    display: flex;
    align-items: flex-start;
    gap: 0.5rem;
  }

  .item-status {
    font-size: 1rem;
    flex-shrink: 0;
  }

  .item-icon {
    font-size: 1rem;
    flex-shrink: 0;
  }

  .item-info {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 0.2rem;
  }

  .item-name {
    font-weight: 500;
    color: var(--text-primary, #fff);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .item-current-step {
    font-size: 0.8rem;
    color: var(--accent, #646cff);
    display: flex;
    align-items: center;
  }

  .item-stats {
    font-size: 0.8rem;
    color: var(--text-secondary, #888);
  }

  .item-destination {
    font-size: 0.75rem;
    color: var(--text-secondary, #888);
    font-family: monospace;
    word-break: break-all;
    display: flex;
    align-items: center;
    gap: 0.5rem;
    flex-wrap: wrap;
  }

  .library-badge {
    background: var(--accent, #646cff);
    color: white;
    padding: 0.1rem 0.4rem;
    border-radius: 4px;
    font-size: 0.65rem;
    font-family: sans-serif;
    text-transform: uppercase;
  }

  .item-error {
    font-size: 0.8rem;
    color: var(--error, #f87171);
  }

  .item-interrupted {
    font-size: 0.8rem;
    color: var(--warning, #fbbf24);
  }

  .result-item.interrupted {
    border-left: 3px solid var(--warning, #fbbf24);
  }

  .item-pending {
    font-size: 0.8rem;
    color: var(--text-secondary, #888);
    font-style: italic;
  }

  .item-actions {
    display: flex;
    gap: 0.25rem;
    align-self: flex-start;
    margin-left: auto;
    flex-shrink: 0;
  }

  .action-btn {
    background: none;
    border: none;
    padding: 0.25rem;
    cursor: pointer;
    font-size: 0.85rem;
    opacity: 0.6;
    transition: opacity 0.15s;
  }

  .action-btn:hover {
    opacity: 1;
  }

  .action-btn.expand {
    font-size: 0.7rem;
    opacity: 0.8;
  }

  /* Journal des étapes */
  .steps-log {
    margin-top: 0.5rem;
    padding: 0.5rem;
    background: rgba(0, 0, 0, 0.3);
    border-radius: 6px;
    font-family: monospace;
    font-size: 0.75rem;
  }

  .steps-header {
    color: var(--text-secondary, #888);
    margin-bottom: 0.5rem;
    font-size: 0.7rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  .steps-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
  }

  .step-item {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem;
    line-height: 1.4;
  }

  .step-time {
    color: var(--text-secondary, #666);
    flex-shrink: 0;
  }

  .step-message {
    color: var(--text-primary, #fff);
  }

  .step-details {
    color: var(--text-secondary, #888);
    font-size: 0.7rem;
  }
</style>


