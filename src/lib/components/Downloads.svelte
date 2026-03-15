<script lang="ts">
  import { t } from '$lib/i18n';
  import { listen } from '@tauri-apps/api/event';
  import { onMount, onDestroy } from 'svelte';
  import { open } from '@tauri-apps/plugin-dialog';
  import {
    parseDownloadLinks,
    startDownloads,
    type DownloadLink,
    type DownloadProgressEvent,
    type DownloadSummary,
    type DownloadStatus,
  } from '$lib/api/commands';

  let linksText = $state('');
  let parsedLinks: DownloadLink[] = $state([]);
  let isParsing = $state(false);
  let isDownloading = $state(false);
  let destDir = $state('');
  let workers = $state(4);
  let summary: DownloadSummary | null = $state(null);
  let progressMap: Map<number, DownloadProgressEvent> = $state(new Map());
  let error: string | null = $state(null);

  let unlistenProgress: (() => void) | null = null;
  let unlistenComplete: (() => void) | null = null;
  let unlistenError: (() => void) | null = null;

  onMount(async () => {
    // Default dest dir
    destDir = 'C:\\Users\\Yanis\\Documents\\DAZ 3D\\Archives';

    unlistenProgress = await listen<DownloadProgressEvent>('download-progress', (event) => {
      const evt = event.payload;
      progressMap = new Map(progressMap).set(evt.index, evt);
    });

    unlistenComplete = await listen<DownloadSummary>('download-complete', (event) => {
      summary = event.payload;
      isDownloading = false;
    });

    unlistenError = await listen<string>('download-error', (event) => {
      error = event.payload;
      isDownloading = false;
    });
  });

  onDestroy(() => {
    unlistenProgress?.();
    unlistenComplete?.();
    unlistenError?.();
  });

  async function handleParse() {
    if (!linksText.trim()) return;
    isParsing = true;
    error = null;
    try {
      parsedLinks = await parseDownloadLinks(linksText);
    } catch (e: any) {
      error = e?.message || String(e);
    } finally {
      isParsing = false;
    }
  }

  async function handleStartDownloads() {
    if (parsedLinks.length === 0 || !destDir.trim()) return;
    isDownloading = true;
    error = null;
    summary = null;
    progressMap = new Map();

    try {
      summary = await startDownloads(parsedLinks, {
        destDir,
        workers,
        retries: 3,
        timeoutSecs: 120,
      });
    } catch (e: any) {
      error = e?.message || String(e);
    } finally {
      isDownloading = false;
    }
  }

  async function handlePickDestDir() {
    const selected = await open({ directory: true, multiple: false });
    if (selected && typeof selected === 'string') {
      destDir = selected;
    }
  }

  function getServiceLabel(service: string): string {
    if (service === 'googledrive') return 'Google Drive';
    if (service === 'mediafire') return 'MediaFire';
    return service;
  }

  function getServiceClass(service: string): string {
    if (service === 'googledrive') return 'badge-gdrive';
    return 'badge-mediafire';
  }

  function getStatusLabel(status: DownloadStatus): string {
    if (status === 'Pending') return $t('downloads.statusPending');
    if (typeof status === 'object') {
      if ('Downloading' in status) {
        const { progressBytes, totalBytes } = status.Downloading;
        if (totalBytes) {
          const pct = Math.round((progressBytes / totalBytes) * 100);
          return `${pct}% (${formatSize(progressBytes)} / ${formatSize(totalBytes)})`;
        }
        return formatSize(progressBytes);
      }
      if ('Completed' in status) return `✅ ${status.Completed.fileName}`;
      if ('Failed' in status) return `❌ ${status.Failed.error}`;
      if ('Skipped' in status) return `⏭️ ${status.Skipped.reason}`;
    }
    return '';
  }

  function getStatusClass(status: DownloadStatus): string {
    if (status === 'Pending') return 'status-pending';
    if (typeof status === 'object') {
      if ('Downloading' in status) return 'status-downloading';
      if ('Completed' in status) return 'status-completed';
      if ('Failed' in status) return 'status-failed';
      if ('Skipped' in status) return 'status-skipped';
    }
    return '';
  }

  function formatSize(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1048576) return `${(bytes / 1024).toFixed(1)} KB`;
    if (bytes < 1073741824) return `${(bytes / 1048576).toFixed(1)} MB`;
    return `${(bytes / 1073741824).toFixed(2)} GB`;
  }

  function getProgressPercent(evt: DownloadProgressEvent): number {
    const s = evt.status;
    if (typeof s === 'object' && 'Downloading' in s && s.Downloading.totalBytes) {
      return Math.round((s.Downloading.progressBytes / s.Downloading.totalBytes) * 100);
    }
    if (typeof s === 'object' && ('Completed' in s)) return 100;
    return 0;
  }
</script>

<div class="downloads-container">
  <!-- Link Input -->
  <div class="section">
    <h3>{$t('downloads.title')}</h3>
    <p class="hint">{$t('downloads.hint')}</p>
    <textarea
      bind:value={linksText}
      placeholder={$t('downloads.placeholder')}
      rows="6"
      disabled={isDownloading}
    ></textarea>
    <button
      class="btn-primary"
      onclick={handleParse}
      disabled={isParsing || isDownloading || !linksText.trim()}
    >
      {isParsing ? $t('downloads.parsing') : $t('downloads.parseButton')}
    </button>
  </div>

  <!-- Parsed Links -->
  {#if parsedLinks.length > 0}
    <div class="section">
      <h3>{$t('downloads.detected', { count: parsedLinks.length })}</h3>
      <ul class="link-list">
        {#each parsedLinks as link, i}
          <li class="link-item">
            <span class="badge {getServiceClass(link.service)}">
              {getServiceLabel(link.service)}
            </span>
            <span class="link-url" title={link.url}>
              {link.url.length > 70 ? link.url.slice(0, 67) + '...' : link.url}
            </span>
            {#if progressMap.has(i)}
              {@const evt = progressMap.get(i)!}
              <span class="link-status {getStatusClass(evt.status)}">
                {getStatusLabel(evt.status)}
              </span>
              {#if getProgressPercent(evt) > 0 && getProgressPercent(evt) < 100}
                <div class="progress-bar">
                  <div class="progress-fill" style="width: {getProgressPercent(evt)}%"></div>
                </div>
              {/if}
            {/if}
          </li>
        {/each}
      </ul>

      <!-- Download Controls -->
      {#if !isDownloading && !summary}
        <div class="controls">
          <div class="dest-row">
            <label>{$t('downloads.destDir')}</label>
            <div class="dest-input">
              <input type="text" bind:value={destDir} readonly />
              <button class="btn-secondary" onclick={handlePickDestDir}>
                {$t('downloads.browse')}
              </button>
            </div>
          </div>
          <div class="workers-row">
            <label>{$t('downloads.workers')}</label>
            <input type="number" bind:value={workers} min="1" max="10" />
          </div>
          <button
            class="btn-primary btn-download"
            onclick={handleStartDownloads}
            disabled={parsedLinks.length === 0 || !destDir.trim()}
          >
            {$t('downloads.startButton', { count: parsedLinks.length })}
          </button>
        </div>
      {/if}

      {#if isDownloading}
        <div class="downloading-info">
          <span class="spinner"></span>
          {$t('downloads.inProgress')}
        </div>
      {/if}
    </div>
  {/if}

  <!-- Summary -->
  {#if summary}
    <div class="section summary">
      <h3>{$t('downloads.summaryTitle')}</h3>
      <div class="summary-grid">
        <div class="summary-stat stat-success">
          <span class="stat-value">{summary.success}</span>
          <span class="stat-label">{$t('downloads.success')}</span>
        </div>
        <div class="summary-stat stat-failed">
          <span class="stat-value">{summary.failed}</span>
          <span class="stat-label">{$t('downloads.failed')}</span>
        </div>
        <div class="summary-stat stat-skipped">
          <span class="stat-value">{summary.skipped}</span>
          <span class="stat-label">{$t('downloads.skipped')}</span>
        </div>
        <div class="summary-stat">
          <span class="stat-value">{formatSize(summary.totalBytes)}</span>
          <span class="stat-label">{$t('downloads.totalSize')}</span>
        </div>
      </div>
      <button class="btn-secondary" onclick={() => { summary = null; parsedLinks = []; linksText = ''; progressMap = new Map(); }}>
        {$t('downloads.newBatch')}
      </button>
    </div>
  {/if}

  <!-- Error -->
  {#if error}
    <div class="error-box">
      <strong>{$t('downloads.error')}</strong> {error}
    </div>
  {/if}
</div>

<style>
  .downloads-container {
    display: flex;
    flex-direction: column;
    gap: 1rem;
  }

  .section {
    background: var(--bg-secondary);
    border-radius: 8px;
    padding: 1.25rem;
  }

  .section h3 {
    margin: 0 0 0.5rem;
    font-size: 1rem;
    color: var(--text-primary);
  }

  .hint {
    color: var(--text-secondary);
    font-size: 0.85rem;
    margin: 0 0 0.75rem;
  }

  textarea {
    width: 100%;
    background: var(--bg-primary);
    color: var(--text-primary);
    border: 1px solid var(--border-color);
    border-radius: 6px;
    padding: 0.75rem;
    font-family: monospace;
    font-size: 0.85rem;
    resize: vertical;
    box-sizing: border-box;
  }

  textarea:focus {
    outline: none;
    border-color: var(--accent);
  }

  .btn-primary {
    margin-top: 0.75rem;
    background: var(--accent);
    color: white;
    padding: 0.6rem 1.5rem;
    border: none;
    border-radius: 6px;
    cursor: pointer;
    font-weight: 500;
  }

  .btn-primary:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .btn-primary:hover:not(:disabled) {
    filter: brightness(1.1);
  }

  .btn-secondary {
    background: var(--bg-tertiary);
    color: var(--text-primary);
    padding: 0.4rem 1rem;
    border: 1px solid var(--border-color);
    border-radius: 6px;
    cursor: pointer;
    font-size: 0.85rem;
  }

  .btn-secondary:hover {
    background: var(--bg-primary);
  }

  .link-list {
    list-style: none;
    margin: 0;
    padding: 0;
  }

  .link-item {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    padding: 0.5rem 0;
    border-bottom: 1px solid var(--border-color);
    flex-wrap: wrap;
  }

  .link-item:last-child {
    border-bottom: none;
  }

  .badge {
    padding: 0.15rem 0.5rem;
    border-radius: 4px;
    font-size: 0.75rem;
    font-weight: 600;
    text-transform: uppercase;
    white-space: nowrap;
  }

  .badge-gdrive {
    background: #1a73e8;
    color: white;
  }

  .badge-mediafire {
    background: #326ce5;
    color: white;
  }

  .link-url {
    flex: 1;
    font-size: 0.8rem;
    font-family: monospace;
    color: var(--text-secondary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
  }

  .link-status {
    font-size: 0.8rem;
    white-space: nowrap;
  }

  .status-pending { color: var(--text-secondary); }
  .status-downloading { color: var(--accent); }
  .status-completed { color: #4caf50; }
  .status-failed { color: #f44336; }
  .status-skipped { color: #ff9800; }

  .progress-bar {
    width: 100%;
    height: 4px;
    background: var(--bg-tertiary);
    border-radius: 2px;
    overflow: hidden;
    flex-basis: 100%;
  }

  .progress-fill {
    height: 100%;
    background: var(--accent);
    transition: width 0.3s ease;
    border-radius: 2px;
  }

  .controls {
    margin-top: 1rem;
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }

  .dest-row, .workers-row {
    display: flex;
    align-items: center;
    gap: 0.75rem;
  }

  .dest-row label, .workers-row label {
    font-size: 0.85rem;
    color: var(--text-secondary);
    white-space: nowrap;
    min-width: 100px;
  }

  .dest-input {
    flex: 1;
    display: flex;
    gap: 0.5rem;
  }

  .dest-input input {
    flex: 1;
    background: var(--bg-primary);
    color: var(--text-primary);
    border: 1px solid var(--border-color);
    border-radius: 6px;
    padding: 0.4rem 0.75rem;
    font-size: 0.85rem;
    font-family: monospace;
  }

  .workers-row input {
    width: 60px;
    background: var(--bg-primary);
    color: var(--text-primary);
    border: 1px solid var(--border-color);
    border-radius: 6px;
    padding: 0.4rem 0.5rem;
    text-align: center;
  }

  .btn-download {
    align-self: flex-start;
  }

  .downloading-info {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    margin-top: 1rem;
    color: var(--accent);
    font-weight: 500;
  }

  .spinner {
    width: 18px;
    height: 18px;
    border: 2px solid var(--bg-tertiary);
    border-top: 2px solid var(--accent);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }

  .summary {
    border-left: 3px solid #4caf50;
  }

  .summary-grid {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 1rem;
    margin: 0.75rem 0;
  }

  .summary-stat {
    text-align: center;
  }

  .stat-value {
    display: block;
    font-size: 1.5rem;
    font-weight: 700;
    color: var(--text-primary);
  }

  .stat-label {
    font-size: 0.8rem;
    color: var(--text-secondary);
  }

  .stat-success .stat-value { color: #4caf50; }
  .stat-failed .stat-value { color: #f44336; }
  .stat-skipped .stat-value { color: #ff9800; }

  .error-box {
    background: #f443361a;
    border: 1px solid #f44336;
    border-radius: 6px;
    padding: 0.75rem 1rem;
    color: #f44336;
    font-size: 0.85rem;
  }
</style>
