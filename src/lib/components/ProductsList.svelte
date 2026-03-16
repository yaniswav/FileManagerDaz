<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { listen, type UnlistenFn } from '@tauri-apps/api/event';
  import { t } from '$lib/i18n';
  import { notify } from '$lib/stores/notifications';
  import {
    listDazLibraries,
    listLibraryProductsPaginated,
    listProductVendors,
    listCollections,
    scanLibraryProducts,
    deleteProduct,
    type DazLibrary,
    type Product,
    type ProductFilters,
    type Collection,
  } from '$lib/api/commands';
  import CustomSelect from '$lib/components/ui/CustomSelect.svelte';
  import ProductCard from '$lib/components/products/ProductCard.svelte';
  import ProductEditDialog from '$lib/components/products/ProductEditDialog.svelte';
  import LibraryStats from '$lib/components/products/LibraryStats.svelte';
  import BatchTagEditor from '$lib/components/products/BatchTagEditor.svelte';
  import CollectionDialog from '$lib/components/products/CollectionDialog.svelte';
  import UninstallDialog from '$lib/components/products/UninstallDialog.svelte';
  import { completedTasks } from '$lib/stores/imports';
  import {
    KNOWN_CONTENT_TYPES,
    type KnownContentType,
  } from '$lib/components/products/utils';

  interface TaskEndPayload {
    id: string;
    task_type: string;
    message: string;
    progress: number | null;
    status: string;
  }

  type SortKey = 'name' | 'date' | 'size';

  const PAGE_SIZE = 50;

  let products: Product[] = $state([]);
  let libraries: DazLibrary[] = $state([]);
  let vendors: string[] = $state([]);
  let total = $state(0);
  let loading = $state(true);
  let loadingMore = $state(false);
  let scanning = $state(false);
  let error: string | null = $state(null);
  let hasMore = $state(true);

  let searchQuery = $state('');
  let contentTypeFilter: string = $state('all');
  let vendorFilter: string = $state('all');
  let sortKey: SortKey = $state('name');
  let selectedLibrary = $state('all');

  let selectedProduct: Product | null = $state(null);
  let sentinelEl: HTMLDivElement | undefined = $state(undefined);
  let searchInputEl: HTMLInputElement | undefined = $state(undefined);
  let resourceProfile: 'low' | 'normal' | 'max' = $state('normal');
  let showStats = $state(false);

  // Multi-select
  let selectedIds: Set<number> = $state(new Set());
  let showTagEditor = $state(false);
  let showCollectionDialog = $state(false);
  let uninstallTarget: Product | null = $state(null);
  let lastClickedIndex: number | null = $state(null);

  // 2D keyboard navigation
  let focusedIndex: number = $state(-1);
  let gridEl: HTMLDivElement | undefined = $state(undefined);
  let cachedCols = 1;

  // Collections
  let collections: Collection[] = $state([]);
  let collectionFilter: string = $state('all');

  function handleCardClick(id: number, event: MouseEvent) {
    const currentIndex = products.findIndex((p) => p.id === id);

    if (event.shiftKey && lastClickedIndex !== null) {
      // Shift+Click: range selection (add range to existing)
      const start = Math.min(lastClickedIndex, currentIndex);
      const end = Math.max(lastClickedIndex, currentIndex);
      for (let i = start; i <= end; i++) {
        selectedIds.add(products[i].id);
      }
    } else if (event.ctrlKey || event.metaKey) {
      // Ctrl+Click: toggle individual without clearing others
      if (selectedIds.has(id)) {
        selectedIds.delete(id);
      } else {
        selectedIds.add(id);
      }
    } else {
      // Plain click: select only this one (deselect all others)
      selectedIds = new Set([id]);
    }

    lastClickedIndex = currentIndex;
    selectedIds = new Set(selectedIds); // trigger Svelte 5 reactivity
  }

  function selectAll() {
    selectedIds = new Set(products.map((p) => p.id));
  }

  function deselectAll() {
    selectedIds = new Set();
    lastClickedIndex = null;
  }

  /** Calculate the number of columns in the CSS grid based on actual rendered width */
  function getGridColumns(): number {
    if (!gridEl || gridEl.children.length === 0) return 1;
    const firstCard = gridEl.children[0] as HTMLElement;
    const gridWidth = gridEl.clientWidth;
    const cardWidth = firstCard.offsetWidth;
    if (cardWidth === 0) return 1;
    // Account for gap
    const style = getComputedStyle(gridEl);
    const gap = parseFloat(style.columnGap || style.gap || '0');
    return Math.max(1, Math.round((gridWidth + gap) / (cardWidth + gap)));
  }

  function updateCachedCols() {
    cachedCols = getGridColumns();
  }

  /** Scroll a card into the visible area of the grid container */
  function scrollCardIntoView(index: number) {
    if (!gridEl || index < 0 || index >= gridEl.children.length) return;
    const card = gridEl.children[index] as HTMLElement;
    card.scrollIntoView({ block: 'nearest', behavior: 'smooth' });
  }

  let debounceTimer: number | null = null;
  let requestId = 0;
  let unlistenTaskEnd: UnlistenFn | null = null;
  let unsubscribeCompleted: (() => void) | null = null;

  onMount(() => {
    void loadLibraries();
    void loadVendors();
    void loadCollectionsData();
    void loadProducts(false);

    // Cache grid column count on resize
    updateCachedCols();
    window.addEventListener('resize', updateCachedCols);

    // Listen for scan task completion via the global task event system
    listen<TaskEndPayload>('app-task-end', (event) => {
      if (event.payload.task_type !== 'scan') return;
      scanning = false;
      if (event.payload.status === 'error') {
        error = event.payload.message;
        notify.error($t('products.scanLibraries'), event.payload.message);
      } else {
        notify.success($t('products.scanLibraries'), event.payload.message);
        void loadVendors(); // refresh vendor list after scan
        resetAndReload();
      }
    }).then((fn) => (unlistenTaskEnd = fn));

    // Auto-refresh when new imports complete
    let prevCompletedCount = 0;
    unsubscribeCompleted = completedTasks.subscribe((tasks) => {
      if (tasks.length > prevCompletedCount && prevCompletedCount > 0) {
        void loadVendors();
        resetAndReload();
      }
      prevCompletedCount = tasks.length;
    });

    // Ctrl+K to focus search
    const handleKeydown = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key === 'k') {
        e.preventDefault();
        searchInputEl?.focus();
      }
      // Ctrl+A: select all products (only when not typing in an input)
      if ((e.ctrlKey || e.metaKey) && e.key === 'a') {
        const tag = (e.target as HTMLElement)?.tagName;
        if (tag === 'INPUT' || tag === 'TEXTAREA') return; // don't hijack text inputs
        e.preventDefault();
        selectAll();
      }
      // Escape: clear selection and focus
      if (e.key === 'Escape') {
        if (selectedIds.size > 0) deselectAll();
        focusedIndex = -1;
      }

      // 2D grid navigation (arrows, Enter, Delete)
      if (['ArrowUp', 'ArrowDown', 'ArrowLeft', 'ArrowRight', 'Enter', 'Delete'].includes(e.key)) {
        const tag = (e.target as HTMLElement)?.tagName;
        if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT') return;
        if (products.length === 0) return;

        if (e.key === 'Enter' && focusedIndex >= 0 && focusedIndex < products.length) {
          e.preventDefault();
          selectedProduct = products[focusedIndex];
          return;
        }
        if (e.key === 'Delete' && focusedIndex >= 0 && focusedIndex < products.length) {
          e.preventDefault();
          uninstallTarget = products[focusedIndex];
          return;
        }

        // Use cached column count for navigation
        const cols = cachedCols;
        let next = focusedIndex;

        if (e.key === 'ArrowRight') next = Math.min(focusedIndex + 1, products.length - 1);
        else if (e.key === 'ArrowLeft') next = Math.max(focusedIndex - 1, 0);
        else if (e.key === 'ArrowDown') next = Math.min(focusedIndex + cols, products.length - 1);
        else if (e.key === 'ArrowUp') next = Math.max(focusedIndex - cols, 0);

        if (next < 0) next = 0;
        if (next !== focusedIndex) {
          e.preventDefault();
          focusedIndex = next;
          selectedIds = new Set([products[next].id]);
          lastClickedIndex = next;
          scrollCardIntoView(next);
        }
      }
    };
    document.addEventListener('keydown', handleKeydown);
    return () => document.removeEventListener('keydown', handleKeydown);
  });

  onDestroy(() => {
    unlistenTaskEnd?.();
    unsubscribeCompleted?.();
    window.removeEventListener('resize', updateCachedCols);
    if (debounceTimer) clearTimeout(debounceTimer);
  });

  // Intersection Observer for infinite scroll
  $effect(() => {
    if (!sentinelEl) return;
    const observer = new IntersectionObserver(
      (entries) => {
        if (entries[0]?.isIntersecting && hasMore && !loadingMore && !loading) {
          void loadMore();
        }
      },
      { rootMargin: '200px' },
    );
    observer.observe(sentinelEl);
    return () => observer.disconnect();
  });

  function buildFilters(offset: number): ProductFilters {
    return {
      limit: PAGE_SIZE,
      offset,
      searchQuery: searchQuery.trim() || undefined,
      libraryFilter: selectedLibrary !== 'all' ? selectedLibrary : undefined,
      typeFilter: contentTypeFilter !== 'all' ? contentTypeFilter : undefined,
      vendorFilter: vendorFilter !== 'all' ? vendorFilter : undefined,
      sortBy: sortKey,
      collectionId: collectionFilter !== 'all' ? Number(collectionFilter) : undefined,
    };
  }

  async function loadProducts(append: boolean): Promise<void> {
    const current = ++requestId;
    if (!append) {
      loading = true;
      error = null;
    } else {
      if (loadingMore) return; // Prevent concurrent loads
      loadingMore = true;
    }

    try {
      const offset = append ? products.length : 0;
      const result = await listLibraryProductsPaginated(buildFilters(offset));

      if (current !== requestId) return;
      if (append) {
        products = [...products, ...result.items];
      } else {
        products = result.items;
      }
      total = result.total;

      // If we received fewer items than requested, there are no more pages
      hasMore = result.items.length >= PAGE_SIZE && products.length < total;
    } catch (e) {
      if (current !== requestId) return;
      error = e instanceof Error ? e.message : String(e);
      hasMore = false;
    } finally {
      if (current === requestId) {
        loading = false;
        loadingMore = false;
      }
    }
  }

  async function loadMore(): Promise<void> {
    if (loadingMore || !hasMore) return;
    await loadProducts(true);
  }

  async function loadLibraries(): Promise<void> {
    try {
      libraries = await listDazLibraries();
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
  }

  async function loadVendors(): Promise<void> {
    try {
      vendors = await listProductVendors();
    } catch {
      // non-critical — vendor dropdown will just be empty
    }
  }

  async function loadCollectionsData(): Promise<void> {
    try {
      collections = await listCollections();
    } catch {
      // non-critical
    }
  }

  function resetAndReload(): void {
    hasMore = true;
    void loadProducts(false);
  }

  function scheduleSearch(): void {
    if (debounceTimer) clearTimeout(debounceTimer);
    debounceTimer = window.setTimeout(() => {
      debounceTimer = null;
      resetAndReload();
    }, 300);
  }

  function handleFilterChange(): void {
    resetAndReload();
  }

  async function handleDelete(id: number): Promise<void> {
    if (!confirm($t('confirm.deleteProduct'))) return;
    try {
      await deleteProduct(id);
      products = products.filter((p) => p.id !== id);
      total = Math.max(0, total - 1);
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
  }

  async function handleScan(): Promise<void> {
    if (scanning) return;
    scanning = true;
    error = null;
    try {
      const libraryPath = selectedLibrary === 'all' ? undefined : selectedLibrary;
      await scanLibraryProducts(libraryPath, resourceProfile);
      // Don't set scanning = false here — wait for 'scan-complete' event
    } catch (e) {
      scanning = false;
      error = e instanceof Error ? e.message : String(e);
    }
  }

  let libraryNameByPath = $derived.by(() => {
    const map = new Map<string, string>();
    for (const lib of libraries) {
      map.set(lib.path, lib.name);
    }
    return map;
  });
</script>

<div class="products-page">
  <!-- Top toolbar -->
  <div class="toolbar">
    <div class="toolbar-row">
      <div class="search-box">
        <svg class="search-icon" viewBox="0 0 20 20" fill="currentColor" width="16" height="16">
          <path fill-rule="evenodd" d="M8 4a4 4 0 100 8 4 4 0 000-8zM2 8a6 6 0 1110.89 3.476l4.817 4.817a1 1 0 01-1.414 1.414l-4.816-4.816A6 6 0 012 8z" clip-rule="evenodd"/>
        </svg>
        <input
          type="text"
          bind:value={searchQuery}
          bind:this={searchInputEl}
          placeholder="{$t('products.search')} (Ctrl+K)"
          oninput={scheduleSearch}
          onkeydown={(e) => e.key === 'Enter' && resetAndReload()}
        />
        {#if searchQuery}
          <button
            type="button"
            class="clear-btn"
            onclick={() => { searchQuery = ''; resetAndReload(); }}
          >✕</button>
        {/if}
      </div>
    </div>

    <div class="toolbar-row">
      <div class="toolbar-filters">
        <CustomSelect
          bind:value={selectedLibrary}
          onchange={() => handleFilterChange()}
          title={$t('products.library')}
          options={[
            { value: 'all', label: $t('products.allLibraries') },
            ...libraries.map(lib => ({ value: lib.path, label: lib.name }))
          ]}
        />

        <CustomSelect
          bind:value={contentTypeFilter}
          onchange={() => handleFilterChange()}
          title={$t('products.contentType')}
          options={[
            { value: 'all', label: $t('common.all') },
            { value: 'unknown', label: $t('common.unknown') },
            ...KNOWN_CONTENT_TYPES.map(ct => ({ value: ct, label: $t(`products.contentTypes.${ct}`) }))
          ]}
        />

        {#if vendors.length > 0}
          <CustomSelect
            bind:value={vendorFilter}
            onchange={() => handleFilterChange()}
            title="Vendor"
            class="select-vendor"
            options={[
              { value: 'all', label: 'All Vendors' },
              ...vendors.map(v => ({ value: v, label: v }))
            ]}
          />
        {/if}

        {#if collections.length > 0}
          <CustomSelect
            bind:value={collectionFilter}
            onchange={() => handleFilterChange()}
            title="Collection"
            class="select-collection"
            options={[
              { value: 'all', label: 'All Collections' },
              ...collections.map(col => ({ value: String(col.id), label: `${col.name} (${col.itemCount})` }))
            ]}
          />
        {/if}

        <CustomSelect
          bind:value={sortKey}
          onchange={() => handleFilterChange()}
          title={$t('products.sortBy')}
          options={[
            { value: 'name', label: $t('products.sortName') },
            { value: 'date', label: $t('products.sortDate') },
            { value: 'size', label: $t('products.sortSize') },
          ]}
        />
      </div>

      <div class="toolbar-actions">
        <button
          type="button"
          class="btn-stats"
          class:active={showStats}
          onclick={() => (showStats = !showStats)}
          title="Dashboard"
        >📊</button>
        <CustomSelect
          bind:value={resourceProfile}
          title="CPU usage"
          class="select-profile"
          disabled={scanning}
          options={[
            { value: 'low', label: '🐢 Low' },
            { value: 'normal', label: '⚡ Normal' },
            { value: 'max', label: '🔥 Max' },
          ]}
        />
        <button type="button" class="btn-scan" onclick={handleScan} disabled={scanning || libraries.length === 0}>
          {scanning ? $t('products.scanning') : $t('products.scanLibraries')}
        </button>
        <button type="button" class="btn-refresh" onclick={resetAndReload} disabled={loading}>
          {$t('common.refresh')}
        </button>
      </div>
    </div>
  </div>

  <!-- Dashboard stats panel (collapsible) -->
  {#if showStats}
    <LibraryStats />
  {/if}

  <!-- Active filters chips -->
  {#if searchQuery || selectedLibrary !== 'all' || contentTypeFilter !== 'all' || vendorFilter !== 'all' || collectionFilter !== 'all'}
    <div class="active-filters">
      {#if searchQuery}
        <span class="chip">
          🔍 {searchQuery}
          <button type="button" onclick={() => { searchQuery = ''; resetAndReload(); }}>✕</button>
        </span>
      {/if}
      {#if selectedLibrary !== 'all'}
        <span class="chip">
          📁 {libraryNameByPath.get(selectedLibrary) ?? selectedLibrary}
          <button type="button" onclick={() => { selectedLibrary = 'all'; handleFilterChange(); }}>✕</button>
        </span>
      {/if}
      {#if contentTypeFilter !== 'all'}
        <span class="chip">
          🏷️ {contentTypeFilter === 'unknown' ? $t('common.unknown') : $t(`products.contentTypes.${contentTypeFilter}`)}
          <button type="button" onclick={() => { contentTypeFilter = 'all'; handleFilterChange(); }}>✕</button>
        </span>
      {/if}
      {#if vendorFilter !== 'all'}
        <span class="chip">
          👤 {vendorFilter}
          <button type="button" onclick={() => { vendorFilter = 'all'; handleFilterChange(); }}>✕</button>
        </span>
      {/if}
      {#if collectionFilter !== 'all'}
        <span class="chip">
          📂 {collections.find(c => String(c.id) === collectionFilter)?.name ?? 'Collection'}
          <button type="button" onclick={() => { collectionFilter = 'all'; handleFilterChange(); }}>✕</button>
        </span>
      {/if}
    </div>
  {/if}

  <!-- Count bar -->
  {#if !loading && !error}
    <div class="count-bar">
      <span class="count-text">
        {$t('products.showingCount').replace('{count}', String(products.length)).replace('{total}', String(total))}
      </span>
    </div>
  {/if}

  <!-- Content area -->
  {#if loading}
    <div class="state-msg">
      <div class="spinner"></div>
      <p>{$t('products.loading')}</p>
    </div>
  {:else if error}
    <div class="error-box">{error}</div>
  {:else if products.length === 0}
    <div class="state-msg">
      <p>{searchQuery ? $t('products.noResults') : $t('products.noProducts')}</p>
      {#if !searchQuery && libraries.length > 0}
        <button type="button" class="btn-scan" onclick={handleScan} disabled={scanning}>
          {scanning ? $t('products.scanning') : $t('products.scanLibraries')}
        </button>
      {/if}
      {#if libraries.length === 0}
        <p class="hint">{$t('products.noLibraries')}</p>
      {/if}
    </div>
  {:else}
    <div class="products-grid" bind:this={gridEl}>
      {#each products as product, i (product.id)}
        <ProductCard
          {product}
          libraryName={libraryNameByPath.get(product.libraryPath ?? '') ?? null}
          selected={selectedIds.has(product.id)}
          focused={focusedIndex === i}
          ondelete={handleDelete}
          onedit={(p) => (selectedProduct = p)}
          onuninstall={(p) => (uninstallTarget = p)}
          onselect={handleCardClick}
        />
      {/each}
    </div>

    <!-- Infinite scroll sentinel -->
    {#if hasMore}
      <div class="sentinel" bind:this={sentinelEl}>
        {#if loadingMore}
          <div class="spinner"></div>
          <span>{$t('products.loadMore')}</span>
        {/if}
      </div>
    {/if}
  {/if}
</div>

{#if selectedProduct}
  <ProductEditDialog
    product={selectedProduct}
    onclose={() => (selectedProduct = null)}
    onsaved={(updated) => {
      products = products.map((p) => (p.id === updated.id ? updated : p));
    }}
    onuninstall={(p) => {
      selectedProduct = null;
      uninstallTarget = p;
    }}
  />
{/if}

<!-- Floating batch action bar -->
{#if selectedIds.size > 1}
  <div class="batch-bar">
    <span class="batch-count">{selectedIds.size} selected</span>
    <button type="button" class="batch-btn" onclick={selectAll}>Select all ({products.length})</button>
    <button type="button" class="batch-btn" onclick={deselectAll}>Deselect all</button>
    <button type="button" class="batch-btn accent" onclick={() => (showTagEditor = true)}>🏷️ Edit Tags</button>
    <button type="button" class="batch-btn accent" onclick={() => (showCollectionDialog = true)}>📂 Add to Collection</button>
    <button type="button" class="batch-btn cancel" onclick={deselectAll}>✕</button>
  </div>
{/if}

{#if showTagEditor}
  <BatchTagEditor
    selectedIds={[...selectedIds]}
    onclose={() => (showTagEditor = false)}
    onapplied={() => { showTagEditor = false; deselectAll(); resetAndReload(); }}
  />
{/if}

{#if showCollectionDialog}
  <CollectionDialog
    productIds={[...selectedIds]}
    onclose={() => (showCollectionDialog = false)}
    onadded={() => { showCollectionDialog = false; deselectAll(); void loadCollectionsData(); }}
  />
{/if}

{#if uninstallTarget}
  <UninstallDialog
    product={uninstallTarget}
    onclose={() => (uninstallTarget = null)}
    onuninstalled={(id) => {
      uninstallTarget = null;
      products = products.filter((p) => p.id !== id);
      total = Math.max(0, total - 1);
    }}
  />
{/if}

<style>
  .products-page {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }

  /* ---- Toolbar ---- */
  .toolbar {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    background: var(--bg-secondary);
    padding: 0.75rem 1rem;
    border-radius: var(--border-radius);
  }

  .toolbar-row {
    display: flex;
    gap: 0.5rem;
    align-items: center;
    flex-wrap: wrap;
  }

  .search-box {
    position: relative;
    flex: 1;
    min-width: 0;
  }

  .search-icon {
    position: absolute;
    left: 10px;
    top: 50%;
    transform: translateY(-50%);
    color: var(--text-secondary);
    pointer-events: none;
  }

  .search-box input {
    width: 100%;
    padding: 0.5rem 2rem 0.5rem 2rem;
    border: 1px solid var(--border-color);
    border-radius: 10px;
    background: var(--bg-primary);
    color: var(--text-primary);
    font-size: 0.9rem;
    box-sizing: border-box;
  }

  .search-box input:focus {
    outline: none;
    border-color: var(--accent);
  }

  .clear-btn {
    position: absolute;
    right: 6px;
    top: 50%;
    transform: translateY(-50%);
    background: none;
    border: none;
    color: var(--text-secondary);
    cursor: pointer;
    font-size: 0.8rem;
    padding: 2px 6px;
    border-radius: 50%;
  }

  .clear-btn:hover {
    background: var(--bg-hover);
  }

  .toolbar-filters {
    display: flex;
    gap: 0.5rem;
    flex-wrap: wrap;
    align-items: center;
    flex: 1;
    min-width: 0;
  }

  .toolbar-filters select {
    padding: 0.45rem 0.6rem;
    border: 1px solid var(--border-color);
    border-radius: 10px;
    background: var(--bg-primary);
    color: var(--text-primary);
    font-size: 0.85rem;
    cursor: pointer;
  }

  .select-vendor {
    max-width: 180px;
  }

  .select-collection {
    max-width: 200px;
  }

  .toolbar-actions {
    display: flex;
    gap: 0.5rem;
    align-items: center;
    margin-left: auto;
    flex-wrap: wrap;
  }

  .btn-scan,
  .btn-refresh {
    padding: 0.45rem 0.8rem;
    border-radius: 10px;
    border: 1px solid var(--border-color);
    cursor: pointer;
    font-size: 0.85rem;
    white-space: nowrap;
  }

  .btn-scan {
    background: var(--accent);
    color: white;
    border-color: var(--accent);
  }

  .btn-refresh {
    background: var(--bg-tertiary);
    color: var(--text-primary);
  }

  .btn-scan:disabled,
  .btn-refresh:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  .select-profile {
    padding: 0.45rem 0.5rem;
    border-radius: 10px;
    border: 1px solid var(--border-color);
    background: var(--bg-tertiary);
    color: var(--text-primary);
    font-size: 0.8rem;
    cursor: pointer;
  }

  .select-profile:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  /* ---- Active filter chips ---- */
  .active-filters {
    display: flex;
    gap: 0.4rem;
    flex-wrap: wrap;
    padding: 0 0.25rem;
  }

  .chip {
    display: inline-flex;
    align-items: center;
    gap: 0.3rem;
    padding: 0.25rem 0.6rem;
    background: rgba(79, 70, 229, 0.15);
    color: var(--accent);
    border-radius: 999px;
    font-size: 0.8rem;
    font-weight: 500;
  }

  .chip button {
    background: none;
    border: none;
    color: inherit;
    cursor: pointer;
    font-size: 0.75rem;
    padding: 0;
    line-height: 1;
    opacity: 0.7;
  }

  .chip button:hover {
    opacity: 1;
  }

  /* ---- Count bar ---- */
  .count-bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 0.25rem;
  }

  .count-text {
    font-size: 0.82rem;
    color: var(--text-secondary);
  }

  /* ---- Products grid ---- */
  .products-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(200px, 1fr));
    gap: 0.75rem;
  }

  /* ---- States ---- */
  .state-msg {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 0.75rem;
    padding: 3rem 1rem;
    color: var(--text-secondary);
    text-align: center;
  }

  .state-msg p {
    margin: 0;
  }

  .hint {
    font-size: 0.85rem;
    opacity: 0.7;
  }

  .error-box {
    padding: 0.75rem;
    background: rgba(233, 69, 96, 0.15);
    border: 1px solid var(--error);
    border-radius: var(--border-radius);
    color: var(--error);
  }

  /* ---- Sentinel / Loading more ---- */
  .sentinel {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 0.5rem;
    padding: 1.5rem 0;
    color: var(--text-secondary);
    font-size: 0.85rem;
  }

  /* ---- Spinner ---- */
  .spinner {
    width: 24px;
    height: 24px;
    border: 3px solid var(--border-color);
    border-top-color: var(--accent);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }

  @media (max-width: 640px) {
    .toolbar {
      flex-direction: column;
      align-items: stretch;
    }

    .toolbar-actions {
      margin-left: 0;
    }

    .products-grid {
      grid-template-columns: repeat(auto-fill, minmax(160px, 1fr));
    }
  }

  /* ---- Stats toggle button ---- */
  .btn-stats {
    background: transparent;
    border: 1px solid var(--border-color, #444);
    border-radius: 6px;
    padding: 0.35rem 0.6rem;
    font-size: 1rem;
    cursor: pointer;
    transition: all 0.15s;
  }
  .btn-stats:hover {
    background: var(--bg-hover, #333);
  }
  .btn-stats.active {
    background: var(--accent-color, #8b5cf6);
    border-color: var(--accent-color, #8b5cf6);
  }

  /* ---- Floating batch action bar ---- */
  .batch-bar {
    position: fixed;
    bottom: 2rem;
    left: 50%;
    transform: translateX(-50%);
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.6rem 1rem;
    background: var(--bg-secondary, #1e1e2f);
    border: 1px solid var(--border-color, #444);
    border-radius: 14px;
    box-shadow: 0 8px 32px rgba(0, 0, 0, 0.4);
    z-index: 100;
    animation: slideUp 0.2s ease;
  }

  @keyframes slideUp {
    from { transform: translateX(-50%) translateY(20px); opacity: 0; }
    to { transform: translateX(-50%) translateY(0); opacity: 1; }
  }

  .batch-count {
    font-size: 0.85rem;
    font-weight: 600;
    color: var(--accent, #8b5cf6);
    padding-right: 0.5rem;
    border-right: 1px solid var(--border-color, #444);
  }

  .batch-btn {
    padding: 0.4rem 0.75rem;
    border-radius: 8px;
    border: 1px solid var(--border-color, #444);
    background: transparent;
    color: var(--text-primary);
    font-size: 0.8rem;
    cursor: pointer;
    white-space: nowrap;
    transition: background 0.12s;
  }

  .batch-btn:hover {
    background: var(--bg-hover, #333);
  }

  .batch-btn.accent {
    background: var(--accent, #8b5cf6);
    color: white;
    border-color: var(--accent, #8b5cf6);
  }

  .batch-btn.accent:hover {
    opacity: 0.9;
  }

  .batch-btn.cancel {
    border: none;
    color: var(--text-secondary);
    font-size: 1rem;
    padding: 0.3rem 0.5rem;
  }
</style>
