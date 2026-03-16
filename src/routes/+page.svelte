<script lang="ts">
  import '../app.css';
  import { onMount, onDestroy } from 'svelte';
  import { listen } from '@tauri-apps/api/event';
  import Status from '$lib/components/Status.svelte';
  import DropZone from '$lib/components/DropZone.svelte';
  import ProductsList from '$lib/components/ProductsList.svelte';
  import Settings from '$lib/components/Settings.svelte';
  import ToolsPanel from '$lib/components/tools/ToolsPanel.svelte';
  import Toast from '$lib/components/Toast.svelte';
  import CloseDialog from '$lib/components/CloseDialog.svelte';
  import StatusBar from '$lib/components/layout/StatusBar.svelte';
  import TaskLogger from '$lib/components/layout/TaskLogger.svelte';
  import { initTaskListeners, destroyTaskListeners } from '$lib/stores/tasks.svelte';
  import { initLogListeners, destroyLogListeners } from '$lib/stores/tasklog.svelte';
  import { initTheme, destroyTheme } from '$lib/stores/theme.svelte';
  import { formatFileSize, getAppConfig, pollWatchEvents, startWatching, getDownloadsFolder, scanWatchedFolder, type RecursiveExtractResult, type WatchEvent } from '$lib/api/commands';
  import { get } from 'svelte/store';
  import { completedTasks, type ImportTask, processMultipleSources } from '$lib/stores/imports';
  import { notify } from '$lib/stores/notifications';
  import { t, initLocale } from '$lib/i18n';
  import { checkForUpdates } from '$lib/api/updater.svelte';

  const WATCHER_POLL_MS = 3_000;
  const CONFIG_REFRESH_MS = 30_000;

  let activeTab: 'extract' | 'products' | 'tools' | 'settings' = $state('extract');
  let productsRefreshKey = $state(0);
  let showCloseDialog = $state(false);
  
  // Reactive state for completed tasks from store
  let recentTasks: ImportTask[] = $state([]);
  $effect(() => {
    recentTasks = get(completedTasks).slice(0, 5);
  });

  let unlistenClose: (() => void) | null = null;
  let watcherPollInterval: ReturnType<typeof setInterval> | null = null;
  let configPollInterval: ReturnType<typeof setInterval> | null = null;
  /** Archives pending user confirmation (for "confirm" mode) */
  let pendingConfirmArchives: { path: string; fileName: string }[] = $state([]);

  /** Handle a detected watcher event according to the configured mode */
  async function handleWatcherEvent(event: WatchEvent, mode: string) {
    if (event.eventType !== 'created') return;

    const fileName = event.fileName || event.path.split(/[\\/]/).pop() || 'archive';

    if (mode === 'auto') {
      // Auto-extract: silently inject into the import pipeline
      notify.info('Auto-Import', `Extracting: ${fileName}`);
      processMultipleSources(
        [event.path],
        undefined,
        (result) => { productsRefreshKey++; },
        (err) => { console.error('[AutoImport] Error:', err); }
      );
    } else if (mode === 'confirm') {
      // Show in-app toast + native notification, wait for user action
      notify.success('Auto-Import', `New archive: ${fileName} — click Extract tab to confirm`);
      pendingConfirmArchives = [...pendingConfirmArchives, { path: event.path, fileName }];
    } else {
      // watch_only: just notify
      notify.success('Auto-Import', `New archive detected: ${fileName}`);
    }
  }

  function confirmExtract(path: string) {
    const archive = pendingConfirmArchives.find(a => a.path === path);
    if (!archive) return;
    pendingConfirmArchives = pendingConfirmArchives.filter(a => a.path !== path);
    notify.info('Auto-Import', `Extracting: ${archive.fileName}`);
    processMultipleSources(
      [archive.path],
      undefined,
      () => { productsRefreshKey++; },
      (err) => { console.error('[AutoImport] Error:', err); }
    );
  }

  function dismissArchive(path: string) {
    pendingConfirmArchives = pendingConfirmArchives.filter(a => a.path !== path);
  }

  // Initialize locale from settings on mount
  onMount(async () => {
    // Start global task event listeners
    await initTaskListeners();
    await initLogListeners();
    initTheme();

    try {
      const config = await getAppConfig();
      initLocale(config.language);

      // Start watcher if auto-import is enabled
      if (config.autoImportEnabled) {
        const folder = config.autoImportFolder || (await getDownloadsFolder()) || '';
        if (folder) {
          try {
            await startWatching(folder);
            const existing = await scanWatchedFolder();
            if (existing.length > 0) {
              notify.success('Auto-Import', `${existing.length} archive(s) detected in watched folder`);
            }
          } catch (e) {
            console.error('Failed to start watcher:', e);
          }
        }
      }

      // Cache the auto-import mode to avoid re-reading config every poll
      let cachedMode = config.autoImportMode || 'watch_only';

      // Start polling for watcher events
      watcherPollInterval = setInterval(async () => {
        try {
          const events: WatchEvent[] = await pollWatchEvents();
          if (events.length === 0) return;
          for (const event of events) {
            await handleWatcherEvent(event, cachedMode);
          }
        } catch {
          // Watcher may not be active, ignore
        }
      }, WATCHER_POLL_MS);

      // Re-read mode periodically (every 30s) in case user changed settings
      configPollInterval = setInterval(async () => {
        try {
          const freshConfig = await getAppConfig();
          cachedMode = freshConfig.autoImportMode || 'watch_only';
        } catch { /* ignore */ }
      }, CONFIG_REFRESH_MS);
    } catch (e) {
      console.error('Failed to load config for i18n:', e);
      initLocale('fr');
    }

    // Listen for close-requested event from backend (when close_action = "ask")
    unlistenClose = await listen('close-requested', () => {
      showCloseDialog = true;
    });

    // Silent update check on startup (no toast if already up-to-date)
    checkForUpdates(true).catch(() => {});
  });

  onDestroy(() => {
    destroyTaskListeners();
    destroyLogListeners();
    destroyTheme();
    unlistenClose?.();
    if (watcherPollInterval) clearInterval(watcherPollInterval);
    if (configPollInterval) clearInterval(configPollInterval);
  });

  async function handleProcessed(result: RecursiveExtractResult) {
    if (import.meta.env.DEV) console.log('[Page] handleProcessed called with:', result);

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
    {#if activeTab === 'extract'}
      <Status />
      <DropZone onprocessed={handleProcessed} onerror={handleError} />

      {#if pendingConfirmArchives.length > 0}
        <div class="pending-archives">
          <h3>📦 {$t('autoImport.pendingTitle')}</h3>
          {#each pendingConfirmArchives as archive (archive.path)}
            <div class="pending-item">
              <span class="pending-name" title={archive.path}>{archive.fileName}</span>
              <button class="btn-confirm" onclick={() => confirmExtract(archive.path)}>
                ✅ {$t('autoImport.extract')}
              </button>
              <button class="btn-dismiss" onclick={() => dismissArchive(archive.path)}>
                ✕
              </button>
            </div>
          {/each}
        </div>
      {/if}

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

<Toast />
<CloseDialog bind:visible={showCloseDialog} />
<TaskLogger />
<StatusBar />

<style>
  main {
    height: 100vh;
    display: flex;
    flex-direction: column;
    padding: 1.5rem;
    padding-bottom: calc(1.5rem + 28px);
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
    max-width: 1400px;
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

  .pending-archives {
    background: var(--bg-secondary);
    border: 1px solid var(--accent);
    border-radius: 8px;
    padding: 1rem;
  }

  .pending-archives h3 {
    margin: 0 0 0.75rem;
    font-size: 1rem;
    color: var(--accent);
  }

  .pending-item {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    padding: 0.5rem 0;
    border-bottom: 1px solid var(--border-color);
  }

  .pending-item:last-child {
    border-bottom: none;
  }

  .pending-name {
    flex: 1;
    font-weight: 500;
    color: var(--text-primary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .btn-confirm {
    background: var(--accent);
    color: white;
    border: none;
    padding: 0.3rem 0.75rem;
    border-radius: 4px;
    cursor: pointer;
    font-size: 0.85rem;
    white-space: nowrap;
  }

  .btn-confirm:hover {
    filter: brightness(1.1);
  }

  .btn-dismiss {
    background: transparent;
    color: var(--text-secondary);
    border: 1px solid var(--border-color);
    padding: 0.3rem 0.5rem;
    border-radius: 4px;
    cursor: pointer;
    font-size: 0.85rem;
  }

  .btn-dismiss:hover {
    color: var(--text-primary);
    border-color: var(--text-secondary);
  }
</style>
