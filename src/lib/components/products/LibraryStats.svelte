<script lang="ts">
  import { onMount } from 'svelte';
  import { t } from '$lib/i18n';
  import {
    getLibraryStats,
    formatFileSize,
    type LibraryStats,
    type Product,
  } from '$lib/api/commands';

  let stats: LibraryStats | null = $state(null);
  let loading = $state(true);
  let error: string | null = $state(null);

  // Maximum count for scaling type bars
  let maxTypeCount = $derived.by(() => {
    if (!stats) return 1;
    const counts = stats.productsByType.map((t) => t.count);
    return Math.max(...counts, 1);
  });

  onMount(() => {
    void loadStats();
  });

  async function loadStats() {
    loading = true;
    error = null;
    try {
      stats = await getLibraryStats();
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      loading = false;
    }
  }

  // Friendly label for content types
  function typeLabel(ct: string): string {
    if (ct === 'unknown') return 'Unknown';
    // Capitalize first letter
    return ct.charAt(0).toUpperCase() + ct.slice(1);
  }

  // Color per content type
  const TYPE_COLORS: Record<string, string> = {
    character: '#8b5cf6',
    clothing: '#f59e0b',
    hair: '#ec4899',
    props: '#10b981',
    pose: '#3b82f6',
    material: '#f97316',
    environment: '#06b6d4',
    light: '#fbbf24',
    shader: '#6366f1',
    script: '#84cc16',
    unknown: '#6b7280',
  };

  function typeColor(ct: string): string {
    return TYPE_COLORS[ct.toLowerCase()] ?? '#6b7280';
  }

  // Initials for product thumbnail placeholder
  function initials(name: string): string {
    return name
      .split(/\s+/)
      .slice(0, 2)
      .map((w) => w[0]?.toUpperCase() ?? '')
      .join('');
  }
</script>

<div class="stats-panel">
  {#if loading}
    <div class="stats-loading">
      <div class="spinner"></div>
    </div>
  {:else if error}
    <div class="stats-error">{error}</div>
  {:else if stats}
    <!-- Summary cards -->
    <div class="stats-cards">
      <div class="stat-card">
        <span class="stat-icon">📦</span>
        <div class="stat-info">
          <span class="stat-value">{stats.totalProducts.toLocaleString()}</span>
          <span class="stat-label">Products</span>
        </div>
      </div>
      <div class="stat-card">
        <span class="stat-icon">💾</span>
        <div class="stat-info">
          <span class="stat-value">{formatFileSize(stats.totalSizeBytes)}</span>
          <span class="stat-label">Library Size</span>
        </div>
      </div>
      <div class="stat-card">
        <span class="stat-icon">🏷️</span>
        <div class="stat-info">
          <span class="stat-value">{stats.productsByType.length}</span>
          <span class="stat-label">Categories</span>
        </div>
      </div>
      <div class="stat-card">
        <span class="stat-icon">👥</span>
        <div class="stat-info">
          <span class="stat-value">{stats.topVendors.length}</span>
          <span class="stat-label">Top Vendors</span>
        </div>
      </div>
    </div>

    <!-- Two columns: types + vendors -->
    <div class="stats-detail">
      <!-- Content type distribution -->
      <div class="stats-section">
        <h4 class="section-title">Content Types</h4>
        <div class="type-bars">
          {#each stats.productsByType as item}
            <div class="type-row">
              <span class="type-label">{typeLabel(item.contentType)}</span>
              <div class="bar-track">
                <div
                  class="bar-fill"
                  style="width: {(item.count / maxTypeCount) * 100}%; background: {typeColor(item.contentType)}"
                ></div>
              </div>
              <span class="type-count">{item.count}</span>
            </div>
          {/each}
        </div>
      </div>

      <!-- Top vendors -->
      <div class="stats-section">
        <h4 class="section-title">Top Vendors</h4>
        <ol class="vendor-list">
          {#each stats.topVendors as v, i}
            <li class="vendor-row">
              <span class="vendor-rank">#{i + 1}</span>
              <span class="vendor-name">{v.vendor}</span>
              <span class="vendor-count">{v.count}</span>
            </li>
          {/each}
        </ol>
      </div>
    </div>

    <!-- Recent products -->
    {#if stats.recentProducts.length > 0}
      <div class="stats-section recent-section">
        <h4 class="section-title">Recently Added</h4>
        <div class="recent-list">
          {#each stats.recentProducts as product}
            <div class="recent-item">
              <span class="recent-avatar">{initials(product.name)}</span>
              <div class="recent-info">
                <span class="recent-name">{product.name}</span>
                <span class="recent-meta">
                  {product.vendor ?? 'Unknown'} · {product.contentType ?? 'Unknown'}
                </span>
              </div>
            </div>
          {/each}
        </div>
      </div>
    {/if}
  {/if}
</div>

<style>
  .stats-panel {
    background: var(--bg-secondary);
    border-radius: var(--border-radius);
    padding: 1rem;
    animation: slideDown 0.2s ease-out;
  }

  @keyframes slideDown {
    from { opacity: 0; transform: translateY(-8px); }
    to { opacity: 1; transform: translateY(0); }
  }

  .stats-loading {
    display: flex;
    justify-content: center;
    padding: 2rem;
  }

  .stats-error {
    color: var(--text-error, #ef4444);
    padding: 0.5rem;
  }

  /* ---- Summary Cards ---- */
  .stats-cards {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(140px, 1fr));
    gap: 0.75rem;
    margin-bottom: 1rem;
  }

  .stat-card {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    background: var(--bg-primary, #1a1a2e);
    border-radius: 10px;
    padding: 0.875rem 1rem;
    border: 1px solid var(--border-color, #333);
  }

  .stat-icon {
    font-size: 1.5rem;
  }

  .stat-info {
    display: flex;
    flex-direction: column;
  }

  .stat-value {
    font-size: 1.25rem;
    font-weight: 700;
    color: var(--text-primary);
    line-height: 1.2;
  }

  .stat-label {
    font-size: 0.75rem;
    color: var(--text-secondary);
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  /* ---- Two-column detail ---- */
  .stats-detail {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 1rem;
    margin-bottom: 1rem;
  }

  @media (max-width: 700px) {
    .stats-detail {
      grid-template-columns: 1fr;
    }
  }

  .stats-section {
    background: var(--bg-primary, #1a1a2e);
    border-radius: 10px;
    padding: 0.875rem 1rem;
    border: 1px solid var(--border-color, #333);
  }

  .section-title {
    margin: 0 0 0.75rem;
    font-size: 0.85rem;
    font-weight: 600;
    color: var(--text-secondary);
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  /* ---- Type bars ---- */
  .type-bars {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  .type-row {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }

  .type-label {
    flex: 0 0 100px;
    font-size: 0.8rem;
    color: var(--text-primary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .bar-track {
    flex: 1;
    height: 8px;
    background: var(--bg-secondary, #2a2a3e);
    border-radius: 4px;
    overflow: hidden;
  }

  .bar-fill {
    height: 100%;
    border-radius: 4px;
    transition: width 0.4s ease;
    min-width: 4px;
  }

  .type-count {
    flex: 0 0 40px;
    text-align: right;
    font-size: 0.8rem;
    color: var(--text-secondary);
    font-variant-numeric: tabular-nums;
  }

  /* ---- Vendor list ---- */
  .vendor-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
  }

  .vendor-row {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.3rem 0;
  }

  .vendor-rank {
    flex: 0 0 28px;
    font-size: 0.75rem;
    color: var(--text-secondary);
    font-weight: 600;
  }

  .vendor-name {
    flex: 1;
    font-size: 0.85rem;
    color: var(--text-primary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .vendor-count {
    flex: 0 0 40px;
    text-align: right;
    font-size: 0.8rem;
    color: var(--text-secondary);
    font-variant-numeric: tabular-nums;
  }

  /* ---- Recent products ---- */
  .recent-section {
    margin-bottom: 0;
  }

  .recent-list {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  .recent-item {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    padding: 0.4rem 0;
  }

  .recent-avatar {
    flex: 0 0 32px;
    height: 32px;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: 6px;
    background: var(--accent-color, #8b5cf6);
    color: #fff;
    font-size: 0.7rem;
    font-weight: 700;
  }

  .recent-info {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }

  .recent-name {
    font-size: 0.85rem;
    color: var(--text-primary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .recent-meta {
    font-size: 0.72rem;
    color: var(--text-secondary);
  }

  .spinner {
    width: 20px;
    height: 20px;
    border: 2px solid var(--border-color, #333);
    border-top-color: var(--accent-color, #8b5cf6);
    border-radius: 50%;
    animation: spin 0.7s linear infinite;
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }
</style>
