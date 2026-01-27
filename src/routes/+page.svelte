<script lang="ts">
  import '../app.css';
  import { onMount } from 'svelte';
  import Status from '$lib/components/Status.svelte';
  import DropZone from '$lib/components/DropZone.svelte';
  import ProductsList from '$lib/components/ProductsList.svelte';
  import Settings from '$lib/components/Settings.svelte';
  import ToolsPanel from '$lib/components/tools/ToolsPanel.svelte';
  import { formatFileSize, getAppConfig, type RecursiveExtractResult } from '$lib/api/commands';
  import { completedTasks, type ImportTask } from '$lib/stores/imports';
  import { t, initLocale } from '$lib/i18n';

  let activeTab: 'extract' | 'products' | 'tools' | 'settings' = $state('extract');
  let productsRefreshKey = $state(0);
  
  // Reactive state for completed tasks from store
  let recentTasks: ImportTask[] = $state([]);
  const unsubscribe = completedTasks.subscribe(tasks => {
    recentTasks = tasks.slice(0, 5); // Last 5
  });

  // Initialize locale from settings on mount
  onMount(async () => {
    try {
      const config = await getAppConfig();
      initLocale(config.language);
    } catch (e) {
      console.error('Failed to load config for i18n:', e);
      initLocale('fr');
    }
  });

  async function handleProcessed(result: RecursiveExtractResult) {
    console.log('[Page] handleProcessed called with:', result);

    // Refresh the products list (products are now created server-side on completion)
    productsRefreshKey++;
  }

  function handleError(data: { path: string; message: string }) {
    console.error('[Page] Processing error:', data);
  }
</script>

<main>
  <header>
    <h1>{$t('app.title')}</h1>
    <p>{$t('app.subtitle')}</p>
  </header>

  <nav class="tabs">
    <button
      class:active={activeTab === 'extract'}
      onclick={() => (activeTab = 'extract')}
    >
      {$t('tabs.import')}
    </button>
    <button
      class:active={activeTab === 'products'}
      onclick={() => (activeTab = 'products')}
    >
      {$t('tabs.products')}
    </button>
    <button
      class:active={activeTab === 'tools'}
      onclick={() => (activeTab = 'tools')}
    >
      {$t('tabs.tools')}
    </button>
    <button
      class:active={activeTab === 'settings'}
      onclick={() => (activeTab = 'settings')}
    >
      {$t('tabs.settings')}
    </button>
  </nav>

  <section class="content">
    {#if activeTab !== 'settings'}
      <Status />
    {/if}

    {#if activeTab === 'extract'}
      <DropZone onprocessed={handleProcessed} onerror={handleError} />

      {#if recentTasks.length > 0}
        <div class="recent-results">
          <h3>{$t('import.recentImports')}</h3>
          <ul>
            {#each recentTasks as task}
              {#if task.result}
                <li class="result-item">
                  <span class="result-name">{task.name}</span>
                  <span class="result-stats">
                    {$t('import.filesImported', { count: task.result.total_files })} - {$t('import.totalSize', { size: formatFileSize(task.result.total_size) })}
                  </span>
                  {#if task.result.analysis?.is_daz_content}
                    <span class="result-type">
                      {$t(`products.contentTypes.${task.result.analysis.content_type}`)}
                    </span>
                  {/if}
                  <span class="result-dest" title={task.result.destination}>
                    {$t('import.destination', { path: task.result.destination.length > 40 ? '...' + task.result.destination.slice(-37) : task.result.destination })}
                  </span>
                </li>
              {/if}
            {/each}
          </ul>
        </div>
      {/if}
    {:else if activeTab === 'products'}
      {#key productsRefreshKey}
        <ProductsList />
      {/key}
    {:else if activeTab === 'tools'}
      <ToolsPanel />
    {:else if activeTab === 'settings'}
      <Settings />
    {/if}
  </section>
</main>

<style>
  main {
    height: 100vh;
    display: flex;
    flex-direction: column;
    padding: 1.5rem;
    overflow: hidden;
  }

  header {
    text-align: center;
    margin-bottom: 1rem;
  }

  header h1 {
    font-size: 2rem;
    color: var(--accent);
  }

  header p {
    color: var(--text-secondary);
    margin-top: 0.25rem;
    font-size: 0.9rem;
  }

  .tabs {
    display: flex;
    gap: 0.5rem;
    margin-bottom: 1rem;
    justify-content: center;
  }

  .tabs button {
    background-color: var(--bg-secondary);
    color: var(--text-secondary);
    padding: 0.5rem 1.5rem;
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }

  .tabs button.active {
    background-color: var(--accent);
    color: white;
  }

  .tabs button:hover:not(.active) {
    background-color: var(--bg-tertiary);
  }

  .content {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 1rem;
    max-width: 900px;
    margin: 0 auto;
    width: 100%;
    overflow-y: auto;
  }

  .recent-results {
    background: var(--bg-secondary);
    border-radius: 8px;
    padding: 1rem;
  }

  .recent-results h3 {
    margin: 0 0 0.75rem;
    font-size: 1rem;
    color: var(--text-primary);
  }

  .recent-results ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }

  .result-item {
    display: flex;
    align-items: center;
    gap: 1rem;
    padding: 0.5rem 0;
    border-bottom: 1px solid var(--border-color);
  }

  .result-item:last-child {
    border-bottom: none;
  }

  .result-name {
    flex: 1;
    font-weight: 500;
    color: var(--text-primary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .result-stats {
    color: var(--text-secondary);
    font-size: 0.85rem;
  }

  .result-type {
    background: var(--accent);
    color: white;
    padding: 0.15rem 0.5rem;
    border-radius: 4px;
    font-size: 0.75rem;
    text-transform: capitalize;
  }

  .result-dest {
    font-size: 0.7rem;
    color: var(--text-secondary);
    font-family: monospace;
    opacity: 0.8;
  }
</style>
