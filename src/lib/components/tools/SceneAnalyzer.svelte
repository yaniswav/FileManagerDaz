<script lang="ts">
  import { open } from '@tauri-apps/plugin-dialog';
  import { t } from '$lib/i18n';
  import {
    analyzeScene,
    formatFileSize,
    type SceneAnalysisReport,
    type RequiredProduct,
  } from '$lib/api/commands';
  import { addToast } from '$lib/stores/toast.svelte';

  interface Props {
    embedded?: boolean;
  }

  let { embedded = false }: Props = $props();

  let filePath = $state<string | null>(null);
  let analyzing = $state(false);
  let report = $state<SceneAnalysisReport | null>(null);
  let error = $state<string | null>(null);
  let dragOver = $state(false);

  async function selectFile() {
    try {
      const selected = await open({
        title: 'Select a DAZ Scene (.duf)',
        filters: [{ name: 'DAZ Scene', extensions: ['duf'] }],
        multiple: false,
        directory: false,
      });
      if (selected) {
        filePath = selected as string;
        await runAnalysis();
      }
    } catch (e: any) {
      error = e?.message ?? String(e);
    }
  }

  async function runAnalysis() {
    if (!filePath) return;
    analyzing = true;
    error = null;
    report = null;
    try {
      report = await analyzeScene(filePath);
      if (report.missingCount === 0) {
        addToast(`All ${report.totalDependencies} assets found!`, 'success', 'Scene Complete');
      } else {
        addToast(
          `${report.missingCount} missing asset${report.missingCount > 1 ? 's' : ''} detected`,
          'warning',
          'Missing Assets'
        );
      }
    } catch (e: any) {
      error = e?.message ?? String(e);
      addToast(error ?? 'Analysis failed', 'error');
    } finally {
      analyzing = false;
    }
  }

  function handleDragOver(e: DragEvent) {
    e.preventDefault();
    dragOver = true;
  }

  function handleDragLeave() {
    dragOver = false;
  }

  function handleDrop(e: DragEvent) {
    e.preventDefault();
    dragOver = false;
    const files = e.dataTransfer?.files;
    if (files && files.length > 0) {
      const file = files[0];
      if (file.name.endsWith('.duf')) {
        // Tauri drag-drop gives the full path via webview
        filePath = (file as any).path ?? file.name;
        void runAnalysis();
      } else {
        addToast('Please drop a .duf scene file', 'error');
      }
    }
  }

  function reset() {
    filePath = null;
    report = null;
    error = null;
  }

  let completionColor = $derived.by(() => {
    if (!report) return '#888';
    if (report.completionPct >= 100) return '#22c55e';
    if (report.completionPct >= 80) return '#f59e0b';
    return '#ef4444';
  });
</script>

<div class="scene-analyzer">
  {#if !report}
    <!-- File Selection / Drop Zone -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="drop-zone"
      class:drag-over={dragOver}
      ondragover={handleDragOver}
      ondragleave={handleDragLeave}
      ondrop={handleDrop}
    >
      {#if analyzing}
        <div class="loading">
          <span class="spinner"></span>
          <p>Analyzing scene…</p>
          <p class="sub">{filePath}</p>
        </div>
      {:else if error}
        <div class="error-block">
          <p>⚠️ {error}</p>
          <button class="btn secondary" onclick={reset}>Try again</button>
        </div>
      {:else}
        <div class="drop-content">
          <span class="drop-icon">🎬</span>
          <h4>Drop a .duf scene file here</h4>
          <p>or click to browse</p>
          <button class="btn primary" onclick={selectFile}>
            📁 Select Scene File
          </button>
        </div>
      {/if}
    </div>
  {:else}
    <!-- Analysis Report -->
    <div class="report">
      <!-- Header -->
      <div class="report-header">
        <div class="header-left">
          <h4 class="scene-name">🎬 {report.sceneName}</h4>
          <p class="scene-path">{filePath}</p>
        </div>
        <button class="btn secondary sm" onclick={reset}>↩ New Analysis</button>
      </div>

      <!-- Completion Gauge -->
      <div class="gauge-section">
        <div class="gauge-ring" style="--pct: {report.completionPct}; --color: {completionColor}">
          <span class="gauge-value" style="color: {completionColor}">
            {report.completionPct.toFixed(0)}%
          </span>
        </div>
        <div class="gauge-info">
          <div class="gauge-label">Scene Completeness</div>
          <div class="gauge-stats">
            <span class="stat-item ok">✓ {report.installedCount} found</span>
            <span class="stat-item missing">✕ {report.missingCount} missing</span>
            <span class="stat-item total">{report.totalDependencies} total dependencies</span>
          </div>
        </div>
      </div>

      <!-- Required Products -->
      {#if report.requiredProducts.length > 0}
        <details class="section installed" open>
          <summary>
            <span class="section-icon">📦</span>
            Required Products ({report.requiredProducts.length})
          </summary>
          <div class="product-list">
            {#each report.requiredProducts as prod}
              <div class="product-row">
                <span class="product-name">{prod.productName}</span>
                <span class="product-files">{prod.filesUsed} file{prod.filesUsed > 1 ? 's' : ''}</span>
              </div>
            {/each}
          </div>
        </details>
      {/if}

      <!-- Untracked Assets (on disk but not in DB) -->
      {#if report.untrackedAssets.length > 0}
        <details class="section untracked">
          <summary>
            <span class="section-icon">📂</span>
            Untracked Assets ({report.untrackedAssets.length})
            <span class="hint">Files found on disk but not linked to any product</span>
          </summary>
          <div class="path-list">
            {#each report.untrackedAssets.slice(0, 100) as path}
              <div class="path-row">{path}</div>
            {/each}
            {#if report.untrackedAssets.length > 100}
              <div class="path-row truncated">… and {report.untrackedAssets.length - 100} more</div>
            {/if}
          </div>
        </details>
      {/if}

      <!-- Missing Assets -->
      {#if report.missingAssets.length > 0}
        <details class="section missing" open>
          <summary>
            <span class="section-icon">❌</span>
            Missing Assets ({report.missingAssets.length})
            <span class="hint">Not found on disk — you may need to install these</span>
          </summary>
          <div class="path-list">
            {#each report.missingAssets.slice(0, 200) as path}
              <div class="path-row">{path}</div>
            {/each}
            {#if report.missingAssets.length > 200}
              <div class="path-row truncated">… and {report.missingAssets.length - 200} more</div>
            {/if}
          </div>
        </details>
      {:else}
        <div class="all-good">
          <span>🎉</span>
          <p>All assets are installed — your scene is ready!</p>
        </div>
      {/if}
    </div>
  {/if}
</div>

<style>
  .scene-analyzer {
    display: flex;
    flex-direction: column;
    gap: 1rem;
  }

  /* ---- Drop Zone ---- */
  .drop-zone {
    border: 2px dashed var(--border-color, #444);
    border-radius: 12px;
    padding: 3rem 2rem;
    display: flex;
    align-items: center;
    justify-content: center;
    text-align: center;
    transition: all 0.2s;
    min-height: 220px;
    background: rgba(255, 255, 255, 0.02);
  }

  .drop-zone.drag-over {
    border-color: var(--accent, #7c3aed);
    background: rgba(124, 58, 237, 0.08);
  }

  .drop-content {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 0.5rem;
  }

  .drop-icon {
    font-size: 3rem;
    margin-bottom: 0.5rem;
  }

  .drop-content h4 {
    margin: 0;
    color: var(--text-primary, #fff);
    font-size: 1.1rem;
  }

  .drop-content p {
    margin: 0;
    color: var(--text-secondary, #888);
    font-size: 0.85rem;
  }

  /* ---- Buttons ---- */
  .btn {
    padding: 0.5rem 1.25rem;
    border-radius: 8px;
    font-size: 0.9rem;
    font-weight: 500;
    cursor: pointer;
    border: none;
    transition: all 0.15s;
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
  }

  .btn.primary {
    background: var(--accent, #7c3aed);
    color: #fff;
    margin-top: 0.75rem;
  }

  .btn.primary:hover {
    filter: brightness(1.15);
  }

  .btn.secondary {
    background: rgba(255, 255, 255, 0.08);
    color: var(--text-primary, #fff);
    border: 1px solid var(--border-color, #444);
  }

  .btn.secondary:hover {
    background: rgba(255, 255, 255, 0.12);
  }

  .btn.sm {
    padding: 0.35rem 0.75rem;
    font-size: 0.8rem;
  }

  /* ---- Loading ---- */
  .loading {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 0.75rem;
    color: var(--text-secondary, #888);
  }

  .loading .sub {
    font-size: 0.75rem;
    opacity: 0.6;
    word-break: break-all;
    max-width: 400px;
  }

  .spinner {
    width: 32px;
    height: 32px;
    border: 3px solid rgba(255, 255, 255, 0.1);
    border-top-color: var(--accent, #7c3aed);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }

  .error-block {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 0.75rem;
    color: #ef4444;
  }

  /* ---- Report ---- */
  .report {
    display: flex;
    flex-direction: column;
    gap: 1rem;
  }

  .report-header {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    gap: 1rem;
  }

  .scene-name {
    margin: 0;
    font-size: 1.15rem;
    color: var(--text-primary, #fff);
  }

  .scene-path {
    margin: 0.25rem 0 0;
    font-size: 0.75rem;
    color: var(--text-secondary, #888);
    word-break: break-all;
    font-family: monospace;
  }

  /* ---- Gauge ---- */
  .gauge-section {
    display: flex;
    align-items: center;
    gap: 1.5rem;
    padding: 1rem 1.25rem;
    background: rgba(255, 255, 255, 0.03);
    border-radius: 10px;
    border: 1px solid rgba(255, 255, 255, 0.06);
  }

  .gauge-ring {
    width: 80px;
    height: 80px;
    border-radius: 50%;
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    background: conic-gradient(
      var(--color) calc(var(--pct) * 1%),
      rgba(255, 255, 255, 0.08) calc(var(--pct) * 1%)
    );
    position: relative;
  }

  .gauge-ring::before {
    content: '';
    position: absolute;
    inset: 6px;
    border-radius: 50%;
    background: var(--bg-secondary, #1a1a2e);
  }

  .gauge-value {
    position: relative;
    z-index: 1;
    font-size: 1.3rem;
    font-weight: 700;
  }

  .gauge-info {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  .gauge-label {
    font-weight: 600;
    font-size: 1rem;
    color: var(--text-primary, #fff);
  }

  .gauge-stats {
    display: flex;
    flex-wrap: wrap;
    gap: 0.75rem;
    font-size: 0.85rem;
  }

  .stat-item.ok {
    color: #22c55e;
  }

  .stat-item.missing {
    color: #ef4444;
  }

  .stat-item.total {
    color: var(--text-secondary, #888);
  }

  /* ---- Sections ---- */
  .section {
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 10px;
    overflow: hidden;
  }

  .section summary {
    padding: 0.75rem 1rem;
    cursor: pointer;
    font-weight: 600;
    font-size: 0.9rem;
    color: var(--text-primary, #fff);
    display: flex;
    align-items: center;
    gap: 0.5rem;
    user-select: none;
  }

  .section summary:hover {
    background: rgba(255, 255, 255, 0.03);
  }

  .section summary .hint {
    font-weight: 400;
    font-size: 0.75rem;
    color: var(--text-secondary, #888);
    margin-left: auto;
  }

  .section.installed {
    border-color: rgba(34, 197, 94, 0.2);
  }

  .section.installed summary {
    background: rgba(34, 197, 94, 0.05);
  }

  .section.untracked {
    border-color: rgba(245, 158, 11, 0.2);
  }

  .section.untracked summary {
    background: rgba(245, 158, 11, 0.05);
  }

  .section.missing {
    border-color: rgba(239, 68, 68, 0.2);
  }

  .section.missing summary {
    background: rgba(239, 68, 68, 0.05);
  }

  .section-icon {
    font-size: 1.1rem;
  }

  /* ---- Product List ---- */
  .product-list {
    display: flex;
    flex-direction: column;
  }

  .product-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 0.5rem 1rem;
    border-top: 1px solid rgba(255, 255, 255, 0.04);
    font-size: 0.85rem;
  }

  .product-row:hover {
    background: rgba(255, 255, 255, 0.02);
  }

  .product-name {
    color: var(--text-primary, #fff);
    font-weight: 500;
  }

  .product-files {
    color: var(--text-secondary, #888);
    font-size: 0.8rem;
  }

  /* ---- Path List ---- */
  .path-list {
    display: flex;
    flex-direction: column;
    max-height: 300px;
    overflow-y: auto;
  }

  .path-row {
    padding: 0.35rem 1rem;
    font-family: monospace;
    font-size: 0.75rem;
    color: var(--text-secondary, #888);
    border-top: 1px solid rgba(255, 255, 255, 0.03);
    word-break: break-all;
  }

  .section.missing .path-row {
    color: #fca5a5;
  }

  .path-row.truncated {
    font-family: inherit;
    font-style: italic;
    color: var(--text-secondary, #888);
  }

  /* ---- All Good ---- */
  .all-good {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    padding: 1rem 1.25rem;
    background: rgba(34, 197, 94, 0.08);
    border: 1px solid rgba(34, 197, 94, 0.2);
    border-radius: 10px;
    font-size: 0.9rem;
    color: #22c55e;
  }

  .all-good span {
    font-size: 1.5rem;
  }

  .all-good p {
    margin: 0;
  }
</style>
