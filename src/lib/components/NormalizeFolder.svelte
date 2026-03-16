<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { listen, type UnlistenFn } from '@tauri-apps/api/event';
  import { open } from '@tauri-apps/plugin-dialog';
  import { t } from '$lib/i18n';
  import {
    type NormalizeBatchResult,
    type NormalizeStepEvent,
    type DazLibrary,
    normalizeBatch,
    listDazLibraries,
    formatFileSize,
  } from '$lib/api/commands';
  import CustomSelect from '$lib/components/ui/CustomSelect.svelte';

  // Props
  interface Props {
    onclose?: () => void;
    embedded?: boolean;
  }

  let { onclose, embedded = false }: Props = $props();

  // States
  let sourceFolder = $state<string | null>(null);
  let libraries: DazLibrary[] = $state([]);
  let selectedLibrary = $state<string>('');
  let loading = $state(false);
  let processing = $state(false);
  let result = $state<NormalizeBatchResult | null>(null);
  let error = $state<string | null>(null);

  // Progress state
  let currentStep = $state<string>('');
  let stepDetail = $state<string>('');

  // Progress event listener
  let unlistenProgress: UnlistenFn | null = null;

  onMount(async () => {
    loading = true;
    try {
      libraries = await listDazLibraries();
      // Select default library
      const defaultLib = libraries.find(l => l.isDefault);
      if (defaultLib) {
        selectedLibrary = defaultLib.path;
      } else if (libraries.length > 0) {
        selectedLibrary = libraries[0].path;
      }

      // Listen for progress events
      unlistenProgress = await listen<NormalizeStepEvent>('normalize-step-normalize', (event) => {
        const data = event.payload;
        currentStep = data.step;
        stepDetail = data.detail ?? '';
      });
    } catch (e) {
      error = e instanceof Error ? e.message : $t('errors.loadError');
    } finally {
      loading = false;
    }
  });

  onDestroy(() => {
    unlistenProgress?.();
  });

  // Source folder selection
  async function selectSourceFolder() {
    try {
      const selected = await open({
        multiple: false,
        directory: true,
        title: $t('tools.normalize.selectSource'),
      });

      if (selected && typeof selected === 'string') {
        sourceFolder = selected;
        result = null;
        error = null;
      }
    } catch (e) {
      console.error('[NormalizeFolder] Folder picker error:', e);
    }
  }

  // Lance la normalisation
  async function handleNormalize() {
    if (!sourceFolder || processing) return;

    processing = true;
    error = null;
    result = null;
    currentStep = 'analyzing';
    stepDetail = $t('tools.normalize.steps.analyzing');

    try {
      result = await normalizeBatch(sourceFolder, selectedLibrary ?? undefined);
    } catch (e) {
      error = e instanceof Error ? e.message : $t('errors.normalizeError');
    } finally {
      processing = false;
      currentStep = '';
      stepDetail = '';
    }
  }

  // Format step name for display (using translate for dynamic step names)
  function formatStep(step: string): string {
    const key = `tools.normalize.stepLabels.${step}`;
    // Fallback to step name if no translation found
    const translated = $t(key);
    return translated !== key ? translated : step;
  }

  // Computed
  let canProcess = $derived(sourceFolder !== null && !processing && !loading);
</script>

<div class="normalize-panel">
  {#if !embedded}
    <header class="panel-header">
      <h2>{$t('tools.normalize.title')}</h2>
      {#if onclose}
        <button type="button" class="close-btn" onclick={onclose} title={$t('common.close')}>X</button>
      {/if}
    </header>
  {/if}

  <div class="panel-content">
    {#if !embedded}
      <p class="description">
        {$t('tools.normalize.description')}
      </p>
    {/if}

    <!-- Source folder selection -->
    <section class="source-section">
      <label for="source-folder">{$t('tools.normalize.sourceFolder')}</label>
      <div class="folder-row">
        <input
          id="source-folder"
          type="text"
          readonly
          value={sourceFolder ?? $t('tools.normalize.noFolder')}
          class:placeholder={!sourceFolder}
        />
        <button type="button" onclick={selectSourceFolder} disabled={processing}>
          {$t('common.browse')}
        </button>
      </div>
    </section>

    <!-- Destination selection -->
    <section class="dest-section">
      <label for="dest-library">{$t('tools.normalize.destLibrary')}</label>
      <CustomSelect
        id="dest-library"
        bind:value={selectedLibrary}
        disabled={processing || loading}
        options={libraries.map(lib => ({ value: lib.path, label: `${lib.name}${lib.isDefault ? ' ' + $t('common.default') : ''}` }))}
      />
    </section>

    <!-- Bouton de lancement -->
    <section class="actions-section">
      <button
        type="button"
        class="btn-primary"
        onclick={handleNormalize}
        disabled={!canProcess}
      >
        {#if processing}
          <span class="spinner"></span> {$t('tools.normalize.processing')}
        {:else}
          {$t('tools.normalize.startBtn')}
        {/if}
      </button>
    </section>

    <!-- Progress bar -->
    {#if processing}
      <div class="progress-section">
        <div class="progress-header">
          <span class="step-label">{formatStep(currentStep)}</span>
        </div>
        <div class="progress-message">{stepDetail}</div>
        <div class="progress-bar indeterminate"></div>
      </div>
    {/if}

    <!-- Erreur -->
    {#if error}
      <div class="error-banner">
        <span>{$t('common.error')}</span> {error}
      </div>
    {/if}

    <!-- Result -->
    {#if result}
      <div class="result-section" class:has-errors={result.errors.length > 0}>
        <h3>{$t('tools.normalize.completed')}</h3>
        
        <div class="result-stats">
          <div class="stat">
            <span class="stat-value">{result.archivesExtracted}</span>
            <span class="stat-label">{$t('tools.normalize.stats.archivesExtracted')}</span>
          </div>
          <div class="stat">
            <span class="stat-value">{result.foldersNormalized}</span>
            <span class="stat-label">{$t('tools.normalize.stats.foldersNormalized')}</span>
          </div>
          <div class="stat">
            <span class="stat-value">{result.foldersMerged}</span>
            <span class="stat-label">{$t('tools.normalize.stats.foldersMerged')}</span>
          </div>
          <div class="stat">
            <span class="stat-value">{result.totalFiles}</span>
            <span class="stat-label">{$t('tools.normalize.stats.filesImported')}</span>
          </div>
          <div class="stat">
            <span class="stat-value">{formatFileSize(result.totalSize)}</span>
            <span class="stat-label">{$t('tools.normalize.stats.totalSize')}</span>
          </div>
        </div>

        {#if result.filesSkipped > 0}
          <div class="skipped-info">
            <h4>{$t('tools.normalize.skippedTitle')} ({result.filesSkipped})</h4>
            <p class="skipped-hint">
              {$t('tools.normalize.skippedHint')}
            </p>
          </div>
        {/if}

        {#if result.errors.length > 0}
          <div class="errors-info">
            <h4>{$t('common.errors')} ({result.errors.length})</h4>
            <ul class="errors-list">
              {#each result.errors.slice(0, 5) as err}
                <li>{err}</li>
              {/each}
              {#if result.errors.length > 5}
                <li class="more">... {$t('tools.normalize.moreErrors', { count: (result.errors.length - 5).toString() })}</li>
              {/if}
            </ul>
          </div>
        {/if}
      </div>
    {/if}
  </div>
</div>

<style>
  .normalize-panel {
    background: var(--bg-secondary);
    border-radius: 8px;
    max-height: 80vh;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
  }

  .panel-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 1rem 1.5rem;
    border-bottom: 1px solid var(--border);
    position: sticky;
    top: 0;
    background: var(--bg-secondary);
    z-index: 10;
  }

  .panel-header h2 {
    margin: 0;
    font-size: 1.25rem;
  }

  .close-btn {
    background: transparent;
    border: none;
    font-size: 1.5rem;
    cursor: pointer;
    color: var(--text-secondary);
    padding: 0.25rem;
    line-height: 1;
  }

  .close-btn:hover {
    color: var(--text);
  }

  .panel-content {
    padding: 1.5rem;
    display: flex;
    flex-direction: column;
    gap: 1.5rem;
  }

  .description {
    color: var(--text-secondary);
    font-size: 0.9rem;
    line-height: 1.5;
    margin: 0;
  }

  section label {
    display: block;
    font-weight: 500;
    margin-bottom: 0.5rem;
    color: var(--text);
  }

  .folder-row {
    display: flex;
    gap: 0.5rem;
  }

  .folder-row input {
    flex: 1;
    padding: 0.5rem 0.75rem;
    border: 1px solid var(--border);
    border-radius: 4px;
    background: var(--bg);
    color: var(--text);
    font-size: 0.9rem;
  }

  .folder-row input.placeholder {
    color: var(--text-secondary);
    font-style: italic;
  }

  .folder-row button {
    padding: 0.5rem 1rem;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 4px;
    cursor: pointer;
    white-space: nowrap;
    color: var(--text);
  }

  .folder-row button:hover:not(:disabled) {
    background: var(--bg-hover);
  }

  select {
    width: 100%;
    padding: 0.5rem 0.75rem;
    border: 1px solid var(--border);
    border-radius: 4px;
    background: var(--bg);
    color: var(--text);
    font-size: 0.9rem;
  }

  .actions-section {
    display: flex;
    justify-content: center;
    padding-top: 0.5rem;
  }

  .btn-primary {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.75rem 2rem;
    background: var(--accent);
    color: white;
    border: none;
    border-radius: 6px;
    font-size: 1rem;
    font-weight: 500;
    cursor: pointer;
    transition: background 0.2s;
  }

  .btn-primary:hover:not(:disabled) {
    background: var(--accent-hover);
  }

  .btn-primary:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  .spinner {
    width: 16px;
    height: 16px;
    border: 2px solid rgba(255, 255, 255, 0.3);
    border-top-color: white;
    border-radius: 50%;
    animation: spin 1s linear infinite;
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }

  .progress-section {
    background: var(--bg);
    border-radius: 6px;
    padding: 1rem;
  }

  .progress-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 0.5rem;
  }

  .step-label {
    font-weight: 500;
  }

  .progress-text {
    color: var(--text-secondary);
    font-size: 0.85rem;
  }

  .progress-message {
    font-size: 0.85rem;
    color: var(--text-secondary);
    margin-bottom: 0.75rem;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .progress-bar {
    height: 6px;
    background: var(--bg-secondary);
    border-radius: 3px;
    overflow: hidden;
  }

  .progress-fill {
    height: 100%;
    background: var(--accent);
    border-radius: 3px;
    transition: width 0.3s ease;
  }

  .progress-bar.indeterminate {
    position: relative;
  }

  .progress-bar.indeterminate::after {
    content: '';
    position: absolute;
    top: 0;
    left: 0;
    height: 100%;
    width: 30%;
    background: var(--accent);
    border-radius: 3px;
    animation: indeterminate 1.5s ease-in-out infinite;
  }

  @keyframes indeterminate {
    0% { left: -30%; }
    100% { left: 100%; }
  }

  .error-banner {
    background: rgba(220, 38, 38, 0.1);
    border: 1px solid rgba(220, 38, 38, 0.3);
    color: #ef4444;
    padding: 0.75rem 1rem;
    border-radius: 6px;
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }

  .result-section {
    background: rgba(34, 197, 94, 0.1);
    border: 1px solid rgba(34, 197, 94, 0.3);
    border-radius: 8px;
    padding: 1.25rem;
  }

  .result-section.has-errors {
    background: rgba(234, 179, 8, 0.1);
    border-color: rgba(234, 179, 8, 0.3);
  }

  .result-section h3 {
    margin: 0 0 1rem 0;
    font-size: 1.1rem;
  }

  .result-stats {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(120px, 1fr));
    gap: 1rem;
    margin-bottom: 1rem;
  }

  .stat {
    text-align: center;
    padding: 0.75rem;
    background: var(--bg);
    border-radius: 6px;
  }

  .stat-value {
    display: block;
    font-size: 1.5rem;
    font-weight: 600;
    color: var(--accent);
  }

  .stat-label {
    display: block;
    font-size: 0.8rem;
    color: var(--text-secondary);
    margin-top: 0.25rem;
  }

  .skipped-info, .errors-info {
    margin-top: 1rem;
    padding-top: 1rem;
    border-top: 1px solid var(--border);
  }

  .skipped-info h4, .errors-info h4 {
    margin: 0 0 0.5rem 0;
    font-size: 0.95rem;
  }

  .skipped-hint {
    font-size: 0.85rem;
    color: var(--text-secondary);
    margin: 0 0 0.75rem 0;
  }

  .skipped-list, .errors-list {
    margin: 0;
    padding-left: 1.25rem;
    font-size: 0.85rem;
    color: var(--text-secondary);
    max-height: 150px;
    overflow-y: auto;
  }

  .skipped-list li, .errors-list li {
    margin-bottom: 0.25rem;
  }

  .more {
    font-style: italic;
    color: var(--text-tertiary);
  }

  .errors-list {
    color: #ef4444;
  }
</style>



