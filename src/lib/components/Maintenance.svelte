<script lang="ts">
  import { onMount } from 'svelte';
  import { t } from '$lib/i18n';
  import {
    type MaintenanceSummary,
    type MaintenanceIssue,
    type CleanupResult,
    type DazLibrary,
    formatFileSize,
    scanLibrary,
    scanAllLibraries,
    cleanupFiles,
    cleanupEmptyFolders,
    quickMaintenanceScan,
    listDazLibraries,
  } from '$lib/api/commands';

  // Props
  interface Props {
    onclose?: () => void;
    embedded?: boolean;
  }

  let { onclose, embedded = false }: Props = $props();

  // States
  let loading = $state(false);
  let scanning = $state(false);
  let cleaning = $state(false);
  let libraries: DazLibrary[] = $state([]);
  let selectedLibrary = $state<string | null>(null);
  let summary: MaintenanceSummary | null = $state(null);
  let cleanupResult: CleanupResult | null = $state(null);
  let error = $state<string | null>(null);

  // Issue selection for cleanup
  let selectedIssues = $state<Set<string>>(new Set());

  // Options de scan
  let scanOptions = $state({
    detectDuplicates: true,
    detectSimilarNames: false,
    detectOrphans: true,
    detectEmptyFolders: true,
    detectTempFiles: true,
  });

  // Load libraries at startup
  onMount(async () => {
    loading = true;
    try {
      libraries = await listDazLibraries();
      if (libraries.length > 0) {
        selectedLibrary = libraries[0].path;
      }
    } catch (e) {
      error = e instanceof Error ? e.message : $t('errors.loadError');
    } finally {
      loading = false;
    }
  });

  // Lance le scan
  async function handleScan() {
    if (scanning) return;
    scanning = true;
    error = null;
    summary = null;
    cleanupResult = null;
    selectedIssues.clear();

    try {
      if (selectedLibrary) {
        summary = await scanLibrary(selectedLibrary, scanOptions);
      } else {
        summary = await scanAllLibraries(scanOptions);
      }
    } catch (e) {
      error = e instanceof Error ? e.message : $t('errors.scanError');
    } finally {
      scanning = false;
    }
  }

  // Scan rapide
  async function handleQuickScan() {
    if (scanning) return;
    scanning = true;
    error = null;
    summary = null;
    cleanupResult = null;
    selectedIssues.clear();

    try {
      summary = await quickMaintenanceScan();
    } catch (e) {
      error = e instanceof Error ? e.message : $t('errors.scanError');
    } finally {
      scanning = false;
    }
  }

  // Select/deselect an issue
  function toggleIssue(path: string) {
    if (selectedIssues.has(path)) {
      selectedIssues.delete(path);
    } else {
      selectedIssues.add(path);
    }
    selectedIssues = new Set(selectedIssues);
  }

  // Select all issues of a type
  function selectAllOfType(type: string) {
    if (!summary) return;
    for (const issue of summary.issues) {
      if (issue.type === type) {
        selectedIssues.add(issue.path);
      }
    }
    selectedIssues = new Set(selectedIssues);
  }

  // Deselect all
  function deselectAll() {
    selectedIssues.clear();
    selectedIssues = new Set(selectedIssues);
  }

  // Clean selected files
  async function handleCleanup(backup: boolean = true) {
    if (cleaning || selectedIssues.size === 0) return;
    cleaning = true;
    error = null;
    cleanupResult = null;

    try {
      const files = Array.from(selectedIssues);
      cleanupResult = await cleanupFiles(files, backup);

      // Remove deleted files from summary
      if (cleanupResult.success && summary) {
        summary.issues = summary.issues.filter(i => !selectedIssues.has(i.path));
        selectedIssues.clear();
        selectedIssues = new Set(selectedIssues);
      }
    } catch (e) {
      error = e instanceof Error ? e.message : $t('errors.cleanupError');
    } finally {
      cleaning = false;
    }
  }

  // Nettoie les dossiers vides
  async function handleCleanEmptyFolders() {
    if (cleaning || !selectedLibrary) return;
    cleaning = true;
    error = null;

    try {
      cleanupResult = await cleanupEmptyFolders(selectedLibrary);
      // Re-run scan to update
      if (cleanupResult.success) {
        await handleScan();
      }
    } catch (e) {
      error = e instanceof Error ? e.message : $t('errors.cleanupError');
    } finally {
      cleaning = false;
    }
  }

  // Formate le type d'issue
  function formatIssueType(type: string): string {
    const key = `tools.maintenance.issueTypes.${type.toLowerCase()}`;
    const translated = $t(key);
    return translated !== key ? translated : type;
  }

  // Icon for issue type
  function getIssueIcon(type: string): string {
    const icons: Record<string, string> = {
      Duplicate: 'DUP',
      SimilarName: 'SIM',
      Orphan: 'ORP',
      EmptyFolder: 'EMP',
      TempFile: 'TMP',
    };
    return icons[type] ?? '';
  }

  // Groupe les issues par type
  function groupIssuesByType(issues: MaintenanceIssue[]): Map<string, MaintenanceIssue[]> {
    const groups = new Map<string, MaintenanceIssue[]>();
    for (const issue of issues) {
      const list = groups.get(issue.type) || [];
      list.push(issue);
      groups.set(issue.type, list);
    }
    return groups;
  }

  // Computed
  let groupedIssues = $derived.by(() => {
    if (!summary) return new Map<string, MaintenanceIssue[]>();
    return groupIssuesByType(summary.issues);
  });
  let hasSelection = $derived(selectedIssues.size > 0);
</script>

<div class="maintenance-panel">
  {#if !embedded}
    <header class="panel-header">
      <h2>{$t('tools.maintenance.title')}</h2>
      {#if onclose}
        <button type="button" class="close-btn" onclick={onclose} title={$t('common.close')}>X</button>
      {/if}
    </header>
  {/if}

  <div class="panel-content">
    {#if !embedded}
      <p class="description">{$t('tools.maintenance.description')}</p>
    {/if}
    <!-- Library selection -->
    <section class="library-section">
      <label for="library-select">{$t('tools.maintenance.libraryToAnalyze')}</label>
      <div class="library-row">
        <select id="library-select" bind:value={selectedLibrary} disabled={scanning || loading}>
          <option value={null}>{$t('tools.maintenance.allLibraries')}</option>
          {#each libraries as lib}
            <option value={lib.path}>
              {lib.name} {lib.isDefault ? $t('common.default') : ''}
            </option>
          {/each}
        </select>
      </div>
    </section>

    <!-- Options de scan -->
    <section class="options-section">
      <h3>{$t('tools.maintenance.scanOptions')}</h3>
      <div class="options-grid">
        <label>
          <input type="checkbox" bind:checked={scanOptions.detectDuplicates} disabled={scanning} />
          {$t('tools.maintenance.options.duplicates')}
        </label>
        <label>
          <input type="checkbox" bind:checked={scanOptions.detectEmptyFolders} disabled={scanning} />
          {$t('tools.maintenance.options.emptyFolders')}
        </label>
        <label>
          <input type="checkbox" bind:checked={scanOptions.detectTempFiles} disabled={scanning} />
          {$t('tools.maintenance.options.tempFiles')}
        </label>
        <label>
          <input type="checkbox" bind:checked={scanOptions.detectOrphans} disabled={scanning} />
          {$t('tools.maintenance.options.orphans')}
        </label>
      </div>
    </section>

    <!-- Boutons d'action -->
    <section class="actions-section">
      <button 
        type="button" 
        class="btn-primary" 
        onclick={handleScan} 
        disabled={scanning || loading}
      >
        {#if scanning}
          <span class="spinner"></span> {$t('tools.maintenance.analyzing')}
        {:else}
          {$t('tools.maintenance.analyze')}
        {/if}
      </button>
      <button 
        type="button" 
        class="btn-secondary" 
        onclick={handleQuickScan}
        disabled={scanning || loading}
      >
        {$t('tools.maintenance.quickScan')}
      </button>
    </section>

    <!-- Erreur -->
    {#if error}
      <div class="error-banner">
        <span>{$t('common.error')}</span> {error}
      </div>
    {/if}

    <!-- Cleanup result -->
    {#if cleanupResult}
      <div class="cleanup-result" class:success={cleanupResult.success} class:failure={!cleanupResult.success}>
        <h4>{cleanupResult.success ? $t('tools.maintenance.cleanupDone') : $t('tools.maintenance.cleanupPartial')}</h4>
        <p>
          {cleanupResult.filesDeleted} {$t('tools.maintenance.filesDeleted')},
          {cleanupResult.foldersDeleted} {$t('tools.maintenance.foldersDeleted')},
          {formatFileSize(cleanupResult.spaceFreed)} {$t('tools.maintenance.spaceFreed')}
        </p>
        {#if cleanupResult.backupPath}
          <p class="backup-info">{$t('tools.maintenance.backup')}: {cleanupResult.backupPath}</p>
        {/if}
        {#if cleanupResult.errors.length > 0}
          <details>
            <summary>{$t('common.errors')} ({cleanupResult.errors.length})</summary>
            <ul>
              {#each cleanupResult.errors as err}
                <li>{err}</li>
              {/each}
            </ul>
          </details>
        {/if}
      </div>
    {/if}

    <!-- Scan summary -->
    {#if summary}
      <section class="summary-section">
        <div class="summary-stats">
          <div class="stat">
            <span class="stat-value">{summary.totalFilesScanned.toLocaleString()}</span>
            <span class="stat-label">{$t('tools.maintenance.filesAnalyzed')}</span>
          </div>
          <div class="stat">
            <span class="stat-value">{formatFileSize(summary.totalSizeScanned)}</span>
            <span class="stat-label">{$t('tools.maintenance.totalSize')}</span>
          </div>
          <div class="stat">
            <span class="stat-value">{summary.issues.length}</span>
            <span class="stat-label">{$t('tools.maintenance.issuesFound')}</span>
          </div>
          <div class="stat recoverable">
            <span class="stat-value">{formatFileSize(summary.recoverableSpace)}</span>
            <span class="stat-label">{$t('tools.maintenance.recoverable')}</span>
          </div>
        </div>
        <p class="scan-time">{$t('tools.maintenance.duration')}: {(summary.scanDurationMs / 1000).toFixed(1)}s</p>
      </section>

      <!-- Grouped issues list -->
      {#if summary.issues.length > 0}
        <section class="issues-section">
          <div class="issues-header">
            <h3>{$t('tools.maintenance.issuesDetected')}</h3>
            <div class="issues-actions">
              {#if hasSelection}
                <button type="button" class="btn-small" onclick={deselectAll}>
                  {$t('tools.maintenance.deselectAll')}
                </button>
              {/if}
            </div>
          </div>

          {#each Array.from(groupedIssues.entries()) as [type, issues]}
            <div class="issue-group">
              <div class="group-header">
                <span class="group-icon">{getIssueIcon(type)}</span>
                <span class="group-title">{formatIssueType(type)}</span>
                <span class="group-count">{issues.length}</span>
                <button 
                  type="button" 
                  class="btn-tiny" 
                  onclick={() => selectAllOfType(type)}
                >
                  {$t('tools.maintenance.selectAll')}
                </button>
              </div>
              <ul class="issues-list">
                {#each issues.slice(0, 50) as issue}
                  <li class="issue-item" class:selected={selectedIssues.has(issue.path)}>
                    <label>
                      <input 
                        type="checkbox" 
                        checked={selectedIssues.has(issue.path)}
                        onchange={() => toggleIssue(issue.path)}
                      />
                      <span class="issue-path" title={issue.path}>
                        {issue.path.split(/[\\/]/).slice(-2).join('/')}
                      </span>
                      {#if issue.size}
                        <span class="issue-size">{formatFileSize(issue.size)}</span>
                      {/if}
                      {#if issue.duplicateOf}
                        <span class="issue-detail">{$t('tools.maintenance.duplicateOf')}: {issue.duplicateOf.split(/[\\/]/).pop()}</span>
                      {/if}
                    </label>
                  </li>
                {/each}
                {#if issues.length > 50}
                  <li class="issue-more">...{$t('common.andMore', { count: (issues.length - 50).toString() })}</li>
                {/if}
              </ul>
            </div>
          {/each}
        </section>

        <!-- Actions de nettoyage -->
        <section class="cleanup-section">
          <h3>{$t('tools.maintenance.cleanup')}</h3>
          <div class="cleanup-actions">
            <button 
              type="button" 
              class="btn-danger" 
              onclick={() => handleCleanup(true)}
              disabled={cleaning || !hasSelection}
            >
              {#if cleaning}
                <span class="spinner"></span>
              {/if}
              {$t('tools.maintenance.deleteWithBackup')} ({selectedIssues.size})
            </button>
            <button 
              type="button" 
              class="btn-warning" 
              onclick={() => handleCleanup(false)}
              disabled={cleaning || !hasSelection}
            >
              {$t('tools.maintenance.deleteNoBackup')}
            </button>
          </div>
          {#if selectedLibrary}
            <button 
              type="button" 
              class="btn-secondary" 
              onclick={handleCleanEmptyFolders}
              disabled={cleaning}
            >
              {$t('tools.maintenance.cleanEmptyFolders')}
            </button>
          {/if}
        </section>
      {:else}
        <div class="no-issues">
          <span class="no-issues-icon">OK</span>
          <p>{$t('tools.maintenance.noIssues')}</p>
        </div>
      {/if}
    {/if}
  </div>
</div>

<style>
  .maintenance-panel {
    background: var(--bg-primary, #0f0f23);
    border: 1px solid var(--border-color, #333);
    border-radius: 12px;
    max-height: 80vh;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .panel-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 1rem 1.5rem;
    border-bottom: 1px solid var(--border-color, #333);
  }

  .panel-header h2 {
    margin: 0;
    font-size: 1.25rem;
  }

  .description {
    margin: 0 0 1rem;
    color: var(--text-secondary, #aaa);
  }

  .close-btn {
    background: none;
    border: none;
    color: var(--text-secondary, #888);
    font-size: 1.25rem;
    cursor: pointer;
    padding: 0.25rem 0.5rem;
    border-radius: 4px;
  }

  .close-btn:hover {
    background: var(--bg-secondary, #1a1a2e);
    color: var(--text-primary, #fff);
  }

  .panel-content {
    flex: 1;
    overflow-y: auto;
    padding: 1.5rem;
    display: flex;
    flex-direction: column;
    gap: 1.5rem;
  }

  /* Sections */
  section h3 {
    margin: 0 0 0.75rem;
    font-size: 0.9rem;
    color: var(--text-secondary, #888);
    font-weight: 500;
  }

  /* Library select */
  .library-row {
    display: flex;
    gap: 0.5rem;
  }

  .library-row select {
    flex: 1;
    padding: 0.5rem;
    background: var(--bg-secondary, #1a1a2e);
    border: 1px solid var(--border-color, #333);
    border-radius: 6px;
    color: var(--text-primary, #fff);
    font-size: 0.9rem;
  }

  /* Options grid */
  .options-grid {
    display: grid;
    grid-template-columns: repeat(2, 1fr);
    gap: 0.5rem;
  }

  .options-grid label {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    font-size: 0.9rem;
    color: var(--text-primary, #fff);
    cursor: pointer;
  }

  /* Actions */
  .actions-section {
    display: flex;
    gap: 0.5rem;
  }

  /* Summary stats */
  .summary-stats {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 1rem;
  }

  .stat {
    display: flex;
    flex-direction: column;
    align-items: center;
    padding: 0.75rem;
    background: var(--bg-secondary, #1a1a2e);
    border-radius: 8px;
  }

  .stat-value {
    font-size: 1.25rem;
    font-weight: 600;
    color: var(--text-primary, #fff);
  }

  .stat-label {
    font-size: 0.75rem;
    color: var(--text-secondary, #888);
  }

  .stat.recoverable .stat-value {
    color: #22c55e;
  }

  .scan-time {
    margin: 0.5rem 0 0;
    font-size: 0.8rem;
    color: var(--text-secondary, #666);
  }

  /* Issues */
  .issues-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 1rem;
  }

  .issue-group {
    margin-bottom: 1rem;
    background: var(--bg-secondary, #1a1a2e);
    border-radius: 8px;
    overflow: hidden;
  }

  .group-header {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.75rem 1rem;
    background: var(--bg-hover, #252545);
  }

  .group-icon {
    font-size: 1.1rem;
  }

  .group-title {
    font-weight: 500;
  }

  .group-count {
    background: var(--accent-color, #6366f1);
    color: white;
    padding: 0.125rem 0.5rem;
    border-radius: 999px;
    font-size: 0.75rem;
    margin-left: auto;
  }

  .issues-list {
    list-style: none;
    margin: 0;
    padding: 0;
    max-height: 200px;
    overflow-y: auto;
  }

  .issue-item {
    padding: 0.5rem 1rem;
    border-bottom: 1px solid var(--border-color, #333);
  }

  .issue-item:last-child {
    border-bottom: none;
  }

  .issue-item.selected {
    background: var(--accent-color, #6366f1)15;
  }

  .issue-item label {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    cursor: pointer;
    font-size: 0.85rem;
  }

  .issue-path {
    flex: 1;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    color: var(--text-primary, #fff);
  }

  .issue-size {
    color: var(--text-secondary, #888);
    font-size: 0.8rem;
  }

  .issue-detail {
    color: var(--text-secondary, #666);
    font-size: 0.75rem;
    font-style: italic;
  }

  .issue-more {
    padding: 0.5rem 1rem;
    color: var(--text-secondary, #666);
    font-size: 0.85rem;
    font-style: italic;
  }

  /* Cleanup */
  .cleanup-section {
    border-top: 1px solid var(--border-color, #333);
    padding-top: 1rem;
  }

  .cleanup-actions {
    display: flex;
    gap: 0.5rem;
    margin-bottom: 0.75rem;
  }

  .cleanup-result {
    padding: 1rem;
    border-radius: 8px;
    margin-bottom: 1rem;
  }

  .cleanup-result.success {
    background: #22c55e20;
    border: 1px solid #22c55e50;
  }

  .cleanup-result.failure {
    background: #f59e0b20;
    border: 1px solid #f59e0b50;
  }

  .cleanup-result h4 {
    margin: 0 0 0.5rem;
  }

  .cleanup-result p {
    margin: 0.25rem 0;
    font-size: 0.9rem;
  }

  .backup-info {
    color: var(--text-secondary, #888);
    font-size: 0.8rem;
  }

  /* No issues */
  .no-issues {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 0.5rem;
    padding: 2rem;
    color: #22c55e;
  }

  .no-issues-icon {
    font-size: 3rem;
  }

  /* Error */
  .error-banner {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.75rem 1rem;
    background: #ef444420;
    border: 1px solid #ef444450;
    border-radius: 8px;
    color: #ef4444;
  }

  /* Buttons */
  .btn-primary, .btn-secondary, .btn-danger, .btn-warning, .btn-small, .btn-tiny {
    padding: 0.5rem 1rem;
    border-radius: 6px;
    font-size: 0.9rem;
    font-weight: 500;
    cursor: pointer;
    transition: all 0.15s ease;
    display: inline-flex;
    align-items: center;
    gap: 0.5rem;
    font-family: inherit;
    border: none;
  }

  .btn-primary {
    background: var(--accent-color, #6366f1);
    color: white;
  }

  .btn-primary:hover:not(:disabled) {
    background: var(--accent-hover, #4f46e5);
  }

  .btn-secondary {
    background: var(--bg-secondary, #1a1a2e);
    border: 1px solid var(--border-color, #333);
    color: var(--text-primary, #fff);
  }

  .btn-secondary:hover:not(:disabled) {
    background: var(--bg-hover, #252545);
  }

  .btn-danger {
    background: #ef4444;
    color: white;
  }

  .btn-danger:hover:not(:disabled) {
    background: #dc2626;
  }

  .btn-warning {
    background: #f59e0b;
    color: black;
  }

  .btn-warning:hover:not(:disabled) {
    background: #d97706;
  }

  .btn-small {
    padding: 0.25rem 0.5rem;
    font-size: 0.8rem;
  }

  .btn-tiny {
    padding: 0.2rem 0.4rem;
    font-size: 0.75rem;
    background: var(--bg-secondary, #1a1a2e);
    color: var(--text-secondary, #888);
  }

  .btn-tiny:hover {
    background: var(--bg-hover, #252545);
    color: var(--text-primary, #fff);
  }

  button:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  /* Spinner */
  .spinner {
    width: 14px;
    height: 14px;
    border: 2px solid currentColor;
    border-top-color: transparent;
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
    display: inline-block;
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }
</style>



