<script lang="ts">
  import { onMount } from 'svelte';
  import { t } from '$lib/i18n';
  import {
    listDazLibraries,
    listLibraryProducts,
    searchLibraryProducts,
    scanLibraryProducts,
    deleteProduct,
    type DazLibrary,
    type Product,
  } from '$lib/api/commands';
  import ProductCard from '$lib/components/products/ProductCard.svelte';
  import ProductEditDialog from '$lib/components/products/ProductEditDialog.svelte';
  import {
    KNOWN_CONTENT_TYPES,
    normalizeContentType,
    type KnownContentType,
  } from '$lib/components/products/utils';

  type SortKey = 'date' | 'name' | 'size';
  type CategoryItem = { path: string; name: string; depth: number; count: number };

  let products: Product[] = $state([]);
  let libraries: DazLibrary[] = $state([]);
  let loading = $state(true);
  let scanning = $state(false);
  let error: string | null = $state(null);

  let searchQuery = $state('');
  let contentTypeFilter: 'all' | 'unknown' | KnownContentType = $state('all');
  let sortKey: SortKey = $state('name');
  let selectedLibrary = $state('all');
  let selectedCategory = $state<string | null>(null);

  let selectedProduct: Product | null = $state(null);

  let debounceTimer: number | null = null;
  let requestId = 0;

  onMount(() => {
    void loadLibraries();
    void refreshProducts();
  });

  async function loadLibraries(): Promise<void> {
    try {
      libraries = await listDazLibraries();
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
  }

  async function refreshProducts(): Promise<void> {
    const current = ++requestId;
    loading = true;
    error = null;

    try {
      const query = searchQuery.trim();
      const data = query ? await searchLibraryProducts(query) : await listLibraryProducts();

      if (current !== requestId) return;
      products = data;
    } catch (e) {
      if (current !== requestId) return;
      error = e instanceof Error ? e.message : String(e);
    } finally {
      if (current === requestId) loading = false;
    }
  }

  function scheduleSearch(): void {
    if (debounceTimer) clearTimeout(debounceTimer);
    debounceTimer = window.setTimeout(() => {
      debounceTimer = null;
      void refreshProducts();
    }, 250);
  }

  async function handleDelete(id: number): Promise<void> {
    if (!confirm($t('confirm.deleteProduct'))) return;

    try {
      await deleteProduct(id);
      await refreshProducts();
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
      await scanLibraryProducts(libraryPath);
      await refreshProducts();
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      scanning = false;
    }
  }

  function normalizeCategoryPath(value: string): string {
    return value.replace(/\\/g, '/').replace(/^\/+|\/+$/g, '');
  }

  function splitCategoryPath(value: string): string[] {
    const normalized = normalizeCategoryPath(value);
    if (!normalized) return [];
    return normalized
      .split('/')
      .map((part) => part.trim())
      .filter(Boolean);
  }

  function buildCategoryItems(items: Product[]): CategoryItem[] {
    const nodes = new Map<string, { name: string; children: Set<string>; count: number }>();

    for (const product of items) {
      const seen = new Set<string>();
      const categories = product.categories ?? [];

      for (const raw of categories) {
        const parts = splitCategoryPath(raw);
        if (parts.length === 0) continue;

        let path = '';
        for (const part of parts) {
          path = path ? `${path}/${part}` : part;
          if (seen.has(path)) continue;
          seen.add(path);

          let node = nodes.get(path);
          if (!node) {
            node = { name: part, children: new Set<string>(), count: 0 };
            nodes.set(path, node);
          }
          node.count += 1;
        }
      }
    }

    for (const path of nodes.keys()) {
      const parentPath = path.includes('/') ? path.slice(0, path.lastIndexOf('/')) : '';
      if (!parentPath) continue;
      const parent = nodes.get(parentPath);
      if (parent) parent.children.add(path);
    }

    const roots = [...nodes.keys()].filter((path) => {
      const parentPath = path.includes('/') ? path.slice(0, path.lastIndexOf('/')) : '';
      return !parentPath || !nodes.has(parentPath);
    });

    const itemsList: CategoryItem[] = [];
    const walk = (path: string, depth: number): void => {
      const node = nodes.get(path);
      if (!node) return;
      itemsList.push({ path, name: node.name, depth, count: node.count });
      const children = [...node.children].sort((a, b) =>
        a.localeCompare(b, undefined, { sensitivity: 'base' })
      );
      for (const child of children) {
        walk(child, depth + 1);
      }
    };

    roots
      .sort((a, b) => a.localeCompare(b, undefined, { sensitivity: 'base' }))
      .forEach((path) => walk(path, 0));

    return itemsList;
  }

  function matchesCategory(product: Product, path: string | null): boolean {
    if (!path) return true;
    const target = normalizeCategoryPath(path);
    if (!target) return true;

    for (const raw of product.categories ?? []) {
      const value = normalizeCategoryPath(raw);
      if (value === target || value.startsWith(`${target}/`)) return true;
    }

    return false;
  }

  let libraryFiltered = $derived.by(() => {
    if (selectedLibrary === 'all') return products;
    return products.filter((product) => (product.libraryPath ?? '') === selectedLibrary);
  });

  let categoryItems = $derived.by(() => buildCategoryItems(libraryFiltered));

  $effect(() => {
    if (selectedCategory && !categoryItems.some((item) => item.path === selectedCategory)) {
      selectedCategory = null;
    }
  });

  let libraryNameByPath = $derived.by(() => {
    const map = new Map<string, string>();
    for (const lib of libraries) {
      map.set(lib.path, lib.name);
    }
    return map;
  });

  let visibleProducts = $derived.by(() => {
    const filtered = libraryFiltered.filter((product) => {
      const normalized = normalizeContentType(product.contentType);
      if (contentTypeFilter === 'unknown') {
        if (normalized !== null) return false;
      } else if (contentTypeFilter !== 'all') {
        if (normalized !== contentTypeFilter) return false;
      }

      if (!matchesCategory(product, selectedCategory)) return false;
      return true;
    });

    const sorted = [...filtered];
    switch (sortKey) {
      case 'name':
        sorted.sort((a, b) => a.name.localeCompare(b.name, undefined, { sensitivity: 'base' }));
        break;
      case 'size':
        sorted.sort((a, b) => (b.totalSize ?? 0) - (a.totalSize ?? 0));
        break;
      case 'date':
      default:
        sorted.sort((a, b) => {
          const at = Date.parse(a.installedAt);
          const bt = Date.parse(b.installedAt);
          return (isNaN(bt) ? 0 : bt) - (isNaN(at) ? 0 : at);
        });
        break;
    }

    return sorted;
  });
</script>

<div class="products-view">
  <aside class="filters">
    <div class="filters-header">
      <div>
        <h3>{$t('products.installed')}</h3>
        <p class="subtitle">{$t('products.libraryCatalog')}</p>
      </div>
      <span class="count">{visibleProducts.length}</span>
    </div>

    <div class="filters-actions">
      <button
        type="button"
        class="btn-primary"
        onclick={handleScan}
        disabled={scanning || libraries.length === 0}
      >
        {scanning ? $t('products.scanning') : $t('products.scanLibraries')}
      </button>
      <button type="button" class="btn-secondary" onclick={refreshProducts} disabled={loading}>
        {$t('common.refresh')}
      </button>
    </div>

    {#if libraries.length === 0}
      <div class="notice">{$t('products.noLibraries')}</div>
    {/if}

    <div class="filter-block">
      <label for="library-filter">{$t('products.library')}</label>
      <select id="library-filter" bind:value={selectedLibrary}>
        <option value="all">{$t('products.allLibraries')}</option>
        {#each libraries as lib}
          <option value={lib.path}>{lib.name}</option>
        {/each}
      </select>
    </div>

    <div class="filter-block">
      <label for="type-filter">{$t('products.contentType')}</label>
      <select id="type-filter" bind:value={contentTypeFilter}>
        <option value="all">{$t('common.all')}</option>
        <option value="unknown">{$t('common.unknown')}</option>
        {#each KNOWN_CONTENT_TYPES as ct}
          <option value={ct}>{$t(`products.contentTypes.${ct}`)}</option>
        {/each}
      </select>
    </div>

    <div class="filter-block">
      <label>{$t('products.categories')}</label>
      <div class="category-list">
        <button
          type="button"
          class:active={!selectedCategory}
          onclick={() => (selectedCategory = null)}
        >
          <span class="category-name">{$t('common.all')}</span>
        </button>
        {#each categoryItems as item}
          <button
            type="button"
            class:active={selectedCategory === item.path}
            style={`padding-left: ${8 + item.depth * 12}px`}
            onclick={() => (selectedCategory = item.path)}
          >
            <span class="category-name">{item.name}</span>
            <span class="category-count">{item.count}</span>
          </button>
        {/each}
      </div>
    </div>
  </aside>

  <section class="products-main">
    <div class="toolbar">
      <input
        type="text"
        bind:value={searchQuery}
        placeholder={$t('products.search')}
        oninput={scheduleSearch}
        onkeydown={(e) => e.key === 'Enter' && refreshProducts()}
      />
      {#if searchQuery}
        <button
          type="button"
          class="inline-btn"
          onclick={() => {
            searchQuery = '';
            refreshProducts();
          }}
        >
          {$t('common.clear')}
        </button>
      {/if}

      <select bind:value={sortKey} title={$t('products.sortBy')}>
        <option value="name">{$t('products.sortName')}</option>
        <option value="date">{$t('products.sortDate')}</option>
        <option value="size">{$t('products.sortSize')}</option>
      </select>
    </div>

    {#if loading}
      <p class="loading">{$t('products.loading')}</p>
    {:else if error}
      <div class="error-box">{error}</div>
    {:else if visibleProducts.length === 0}
      <div class="empty">
        <p>{searchQuery ? $t('products.noResults') : $t('products.noProducts')}</p>
        {#if !searchQuery && libraries.length > 0}
          <button type="button" class="btn-primary" onclick={handleScan} disabled={scanning}>
            {scanning ? $t('products.scanning') : $t('products.scanLibraries')}
          </button>
        {/if}
      </div>
    {:else}
      <div class="products-grid">
        {#each visibleProducts as product (product.id)}
          <ProductCard
            product={product}
            libraryName={libraryNameByPath.get(product.libraryPath ?? '') ?? product.libraryPath ?? null}
            ondelete={handleDelete}
            onedit={(p) => (selectedProduct = p)}
          />
        {/each}
      </div>
    {/if}
  </section>
</div>

{#if selectedProduct}
  <ProductEditDialog
    product={selectedProduct}
    onclose={() => (selectedProduct = null)}
    onsaved={(updated) => {
      products = products.map((p) => (p.id === updated.id ? updated : p));
      if (searchQuery.trim()) void refreshProducts();
    }}
  />
{/if}

<style>
  .products-view {
    display: grid;
    grid-template-columns: minmax(220px, 280px) 1fr;
    gap: 1.25rem;
  }

  .filters {
    background-color: var(--bg-secondary);
    border-radius: var(--border-radius);
    padding: 1rem;
    display: flex;
    flex-direction: column;
    gap: 1rem;
    height: fit-content;
  }

  .filters-header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 0.75rem;
  }

  .filters-header h3 {
    margin: 0;
    color: var(--accent);
  }

  .subtitle {
    margin: 0.25rem 0 0;
    font-size: 0.85rem;
    color: var(--text-secondary);
  }

  .count {
    font-size: 1.1rem;
    font-weight: 700;
    color: var(--text-primary);
  }

  .filters-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem;
  }

  .btn-primary,
  .btn-secondary {
    padding: 0.5rem 0.8rem;
    border-radius: 10px;
    border: 1px solid var(--border-color);
    cursor: pointer;
    font-size: 0.85rem;
  }

  .btn-primary {
    background: var(--accent);
    color: white;
    border-color: var(--accent);
  }

  .btn-secondary {
    background: var(--bg-tertiary);
    color: var(--text-primary);
  }

  .btn-primary:disabled,
  .btn-secondary:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  .notice {
    padding: 0.6rem 0.75rem;
    border-radius: 10px;
    background: var(--bg-tertiary);
    color: var(--text-secondary);
    font-size: 0.85rem;
  }

  .filter-block {
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
  }

  .filter-block label {
    font-size: 0.85rem;
    color: var(--text-secondary);
  }

  select,
  .toolbar input {
    padding: 0.45rem 0.6rem;
    border: 1px solid var(--border-color);
    border-radius: var(--border-radius);
    background-color: var(--bg-primary);
    color: var(--text-primary);
    font-size: 0.9rem;
  }

  .category-list {
    border: 1px solid var(--border-color);
    background: var(--bg-primary);
    border-radius: 12px;
    padding: 0.3rem;
    max-height: 320px;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 0.2rem;
  }

  .category-list button {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 0.5rem;
    border: none;
    background: transparent;
    color: var(--text-primary);
    padding: 0.3rem 0.4rem;
    border-radius: 8px;
    cursor: pointer;
    text-align: left;
  }

  .category-list button:hover {
    background: var(--bg-hover);
  }

  .category-list button.active {
    background: rgba(79, 70, 229, 0.2);
    color: var(--accent);
    font-weight: 600;
  }

  .category-count {
    font-size: 0.75rem;
    color: var(--text-secondary);
  }

  .products-main {
    background-color: var(--bg-secondary);
    border-radius: var(--border-radius);
    padding: 1.25rem;
    display: flex;
    flex-direction: column;
    gap: 1rem;
    min-width: 0;
  }

  .toolbar {
    display: flex;
    gap: 0.5rem;
    align-items: center;
    flex-wrap: wrap;
  }

  .toolbar input {
    flex: 1;
    min-width: 180px;
  }

  .inline-btn {
    padding: 0.45rem 0.7rem;
    border-radius: 10px;
    border: 1px solid var(--border-color);
    background: var(--bg-tertiary);
    color: var(--text-primary);
    cursor: pointer;
  }

  .inline-btn:hover {
    background: var(--bg-hover);
  }

  .loading,
  .empty {
    color: var(--text-secondary);
    text-align: center;
    padding: 2rem 1rem;
  }

  .empty p {
    margin: 0 0 1rem;
  }

  .error-box {
    padding: 0.75rem;
    background-color: rgba(233, 69, 96, 0.2);
    border: 1px solid var(--error);
    border-radius: var(--border-radius);
    color: var(--error);
  }

  .products-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(260px, 1fr));
    gap: 0.9rem;
  }

  @media (max-width: 920px) {
    .products-view {
      grid-template-columns: 1fr;
    }

    .filters {
      position: static;
    }
  }
</style>
