<script lang="ts">
  import { t } from '$lib/i18n';
  import {
    findDuplicates,
    deleteProduct,
    formatFileSize,
    type DuplicateGroup,
    type Product,
  } from '$lib/api/commands';
  import { open } from '@tauri-apps/plugin-shell';

  interface Props {
    embedded?: boolean;
  }

  let { embedded = false }: Props = $props();

  let groups: DuplicateGroup[] = $state([]);
  let loading = $state(false);
  let error: string | null = $state(null);
  let scanned = $state(false);

  let totalDuplicates = $derived(groups.reduce((sum, g) => sum + g.count - 1, 0));

  async function scan() {
    loading = true;
    error = null;
    try {
      groups = await findDuplicates();
      scanned = true;
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      loading = false;
    }
  }

  async function handleOpenFolder(path: string) {
    try {
      await open(path);
    } catch (e) {
      console.warn('[Duplicates] Failed to open:', path, e);
    }
  }

  async function handleDelete(product: Product, group: DuplicateGroup) {
    if (!confirm(`Delete "${product.name}" at ${product.path}?`)) return;
    try {
      await deleteProduct(product.id);
      // Remove from group
      group.products = group.products.filter((p) => p.id !== product.id);
      group.count = group.products.length;
      // Remove group if only 1 left
      if (group.products.length <= 1) {
        groups = groups.filter((g) => g !== group);
      }
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
  }
</script>

<div class="duplicates-panel">
  {#if !scanned}
    <div class="scan-prompt">
      <p class="scan-desc">Scan your library to find products that share the same name and vendor.</p>
      <button type="button" class="btn-scan" onclick={scan} disabled={loading}>
        {#if loading}
          <span class="spinner"></span> Scanning…
        {:else}
          🔍 Scan for Duplicates
        {/if}
      </button>
    </div>
  {:else if error}
    <div class="error-box">{error}</div>
  {:else if groups.length === 0}
    <div class="empty-state">
      <span class="empty-icon">✅</span>
      <p>No duplicates found! Your library is clean.</p>
      <button type="button" class="btn-rescan" onclick={scan} disabled={loading}>Re-scan</button>
    </div>
  {:else}
    <div class="results-header">
      <span class="results-summary">
        Found <strong>{totalDuplicates}</strong> duplicate(s) across <strong>{groups.length}</strong> group(s)
      </span>
      <button type="button" class="btn-rescan" onclick={scan} disabled={loading}>🔄 Re-scan</button>
    </div>

    <div class="groups-list">
      {#each groups as group (group.name + (group.vendor ?? ''))}
        <details class="dup-group" open>
          <summary class="group-header">
            <span class="group-name">{group.name}</span>
            {#if group.vendor}
              <span class="group-vendor">by {group.vendor}</span>
            {/if}
            <span class="group-count">{group.count} copies</span>
          </summary>

          <div class="group-items">
            {#each group.products as product, i (product.id)}
              <div class="dup-item" class:original={i === 0}>
                <div class="item-info">
                  {#if i === 0}
                    <span class="item-badge original-badge">Original</span>
                  {:else}
                    <span class="item-badge dup-badge">Duplicate</span>
                  {/if}
                  <span class="item-path" title={product.path}>{product.path}</span>
                  {#if product.totalSize > 0}
                    <span class="item-size">{formatFileSize(product.totalSize)}</span>
                  {/if}
                </div>
                <div class="item-actions">
                  <button
                    type="button"
                    class="action-btn"
                    title="Open folder"
                    onclick={() => handleOpenFolder(product.path)}
                  >📁</button>
                  {#if i > 0}
                    <button
                      type="button"
                      class="action-btn danger"
                      title="Delete from library"
                      onclick={() => handleDelete(product, group)}
                    >🗑️</button>
                  {/if}
                </div>
              </div>
            {/each}
          </div>
        </details>
      {/each}
    </div>
  {/if}
</div>

<style>
  .duplicates-panel {
    display: flex;
    flex-direction: column;
    gap: 1rem;
  }

  /* ---- Scan prompt ---- */
  .scan-prompt {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 1rem;
    padding: 2rem 1rem;
    text-align: center;
  }

  .scan-desc {
    color: var(--text-secondary);
    font-size: 0.9rem;
    margin: 0;
  }

  .btn-scan {
    display: inline-flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.6rem 1.2rem;
    border-radius: 10px;
    border: none;
    background: var(--accent, #8b5cf6);
    color: white;
    font-size: 0.9rem;
    font-weight: 600;
    cursor: pointer;
    transition: opacity 0.15s;
  }

  .btn-scan:hover { opacity: 0.9; }
  .btn-scan:disabled { opacity: 0.6; cursor: not-allowed; }

  .btn-rescan {
    padding: 0.4rem 0.8rem;
    border-radius: 8px;
    border: 1px solid var(--border-color);
    background: var(--bg-tertiary);
    color: var(--text-primary);
    font-size: 0.82rem;
    cursor: pointer;
  }

  .btn-rescan:hover { background: var(--bg-hover); }

  /* ---- Empty state ---- */
  .empty-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 0.75rem;
    padding: 2rem;
    text-align: center;
    color: var(--text-secondary);
  }

  .empty-icon { font-size: 2rem; }

  .empty-state p { margin: 0; }

  /* ---- Results header ---- */
  .results-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    flex-wrap: wrap;
    gap: 0.5rem;
  }

  .results-summary {
    font-size: 0.9rem;
    color: var(--text-primary);
  }

  .error-box {
    padding: 0.75rem 1rem;
    border-radius: 8px;
    background: rgba(239, 68, 68, 0.1);
    color: #ef4444;
    font-size: 0.85rem;
  }

  /* ---- Groups ---- */
  .groups-list {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }

  .dup-group {
    background: var(--bg-primary, #1a1a2e);
    border: 1px solid var(--border-color, #333);
    border-radius: 10px;
    overflow: hidden;
  }

  .group-header {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    padding: 0.75rem 1rem;
    cursor: pointer;
    user-select: none;
    font-size: 0.9rem;
  }

  .group-header:hover {
    background: var(--bg-hover);
  }

  .group-name {
    font-weight: 600;
    color: var(--text-primary);
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .group-vendor {
    font-size: 0.8rem;
    color: var(--text-secondary);
    flex-shrink: 0;
  }

  .group-count {
    font-size: 0.75rem;
    color: var(--accent, #8b5cf6);
    font-weight: 600;
    background: rgba(139, 92, 246, 0.15);
    padding: 2px 8px;
    border-radius: 999px;
    flex-shrink: 0;
  }

  /* ---- Items ---- */
  .group-items {
    border-top: 1px solid var(--border-color, #333);
  }

  .dup-item {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 0.5rem;
    padding: 0.6rem 1rem;
    font-size: 0.82rem;
  }

  .dup-item:not(:last-child) {
    border-bottom: 1px solid rgba(255, 255, 255, 0.05);
  }

  .dup-item.original {
    background: rgba(16, 185, 129, 0.05);
  }

  .item-info {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    min-width: 0;
    flex: 1;
  }

  .item-badge {
    font-size: 0.7rem;
    font-weight: 600;
    padding: 1px 6px;
    border-radius: 4px;
    flex-shrink: 0;
    text-transform: uppercase;
    letter-spacing: 0.03em;
  }

  .original-badge {
    background: rgba(16, 185, 129, 0.2);
    color: #10b981;
  }

  .dup-badge {
    background: rgba(245, 158, 11, 0.2);
    color: #f59e0b;
  }

  .item-path {
    color: var(--text-secondary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
  }

  .item-size {
    color: var(--text-secondary);
    font-size: 0.75rem;
    flex-shrink: 0;
  }

  .item-actions {
    display: flex;
    gap: 0.3rem;
    flex-shrink: 0;
  }

  .action-btn {
    width: 28px;
    height: 28px;
    border-radius: 6px;
    border: 1px solid var(--border-color, #333);
    background: transparent;
    cursor: pointer;
    display: grid;
    place-items: center;
    font-size: 0.85rem;
    transition: background 0.12s;
  }

  .action-btn:hover {
    background: var(--bg-hover);
  }

  .action-btn.danger:hover {
    background: rgba(239, 68, 68, 0.15);
  }

  .spinner {
    width: 16px;
    height: 16px;
    border: 2px solid rgba(255, 255, 255, 0.3);
    border-top-color: white;
    border-radius: 50%;
    animation: spin 0.7s linear infinite;
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }
</style>
