<script lang="ts">
  import { t } from '$lib/i18n';
  import { formatFileSize, parseTags, type Product } from '$lib/api/commands';
  import { open } from '@tauri-apps/plugin-shell';
  import { convertFileSrc } from '@tauri-apps/api/core';
  import { getContentTypeIcon, getContentTypeLabel, normalizeContentType } from './utils';

  interface Props {
    product: Product;
    libraryName?: string | null;
    selected?: boolean;
    focused?: boolean;
    ondelete?: (id: number) => void;
    onedit?: (product: Product) => void;
    onuninstall?: (product: Product) => void;
    onselect?: (id: number, event: MouseEvent) => void;
  }

  let { product, libraryName = null, selected = false, focused = false, ondelete, onedit, onuninstall, onselect }: Props = $props();

  let showMenu = $state(false);
  let imgFailed = $state(false);

  let thumbnailSrc = $derived.by(() => {
    const raw = product.thumbnailPath;
    if (!raw || raw.trim() === '') return null;
    const src = convertFileSrc(raw);
    return src;
  });

  let tagInfo = $derived.by(() => {
    const tags = parseTags(product.tags ?? '');
    const visible = tags.slice(0, 3);
    return { visible, extra: Math.max(0, tags.length - visible.length) };
  });

  /** Resolve the full path to a launchable .duf file (if any). */
  let dufPath = $derived.by(() => {
    const sf = product.supportFile;
    if (!sf || sf.startsWith('__custom__')) return null;
    // supportFile is library-relative — rebuild absolute path
    if (sf.toLowerCase().endsWith('.duf') && product.libraryPath) {
      return `${product.libraryPath}/${sf}`.replace(/\\/g, '/');
    }
    // For products with a real .duf in their path
    const p = product.path;
    if (p && p.toLowerCase().endsWith('.duf')) return p.replace(/\\/g, '/');
    return null;
  });

  async function handleOpenFolder(e: MouseEvent): Promise<void> {
    e.stopPropagation();
    try {
      await open(product.path);
    } catch (err) {
      console.warn('[ProductCard] Failed to open folder:', product.path, err);
    }
  }

  async function handleOpenInDaz(e: MouseEvent): Promise<void> {
    e.stopPropagation();
    if (!dufPath) return;
    try {
      await open(dufPath);
    } catch (err) {
      console.warn('[ProductCard] Failed to open in DAZ:', dufPath, err);
    }
  }

  async function openInExplorer(path: string): Promise<void> {
    try {
      await open(path);
    } catch (e) {
      console.warn('[Products] Failed to open path:', path, e);
    }
  }

  function handleMenuToggle(e: MouseEvent): void {
    e.stopPropagation();
    showMenu = !showMenu;
  }

  function handleAction(action: () => void): void {
    showMenu = false;
    action();
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<!-- svelte-ignore a11y_click_events_have_key_events -->
<div
  class="card"
  class:selected
  class:focused
  onmouseleave={() => (showMenu = false)}
  onclick={(e: MouseEvent) => {
    // Don't trigger selection if clicking on menu/actions
    const target = e.target as HTMLElement;
    if (target.closest('.hover-actions') || target.closest('.quick-actions') || target.closest('.dropdown')) return;
    onselect?.(product.id, e);
  }}
  ondblclick={(e: MouseEvent) => {
    const target = e.target as HTMLElement;
    if (target.closest('.hover-actions') || target.closest('.quick-actions') || target.closest('.dropdown')) return;
    e.preventDefault();
    onedit?.(product);
  }}
>
  <!-- Thumbnail area -->
  <div class="card-thumb">
    {#if thumbnailSrc && !imgFailed}
      <img
        src={thumbnailSrc}
        alt={product.name}
        loading="lazy"
        onerror={() => (imgFailed = true)}
      />
    {:else}
      <div class="thumb-fallback">
        <span class="thumb-icon-text">{getContentTypeIcon(product.contentType)}</span>
      </div>
    {/if}

    <!-- Overlay badges -->
    <div class="badges-overlay">
      {#if product.contentType}
        <span class="badge type-badge">{getContentTypeLabel(product.contentType, $t)}</span>
      {/if}
      {#if libraryName}
        <span class="badge lib-badge">{libraryName}</span>
      {/if}
    </div>

    <!-- Hover actions (top-right menu) -->
    <div class="hover-actions">
      <button
        type="button"
        class="menu-btn"
        title="Actions"
        aria-label="Actions"
        onclick={handleMenuToggle}
      >⋯</button>

      {#if showMenu}
        <div class="dropdown">
          <button type="button" onclick={() => handleAction(() => void openInExplorer(product.path))}>
            📁 {$t('products.openFolder')}
          </button>
          {#if dufPath}
            <button type="button" onclick={() => handleAction(() => void openInExplorer(dufPath!))}>
              🚀 Open in DAZ
            </button>
          {/if}
          <button type="button" onclick={() => handleAction(() => onedit?.(product))}>
            ✏️ {$t('common.edit')}
          </button>
          <button type="button" class="danger" onclick={() => handleAction(() => onuninstall?.(product))}>
            🗑️ Uninstall
          </button>
          <button type="button" class="danger" onclick={() => handleAction(() => ondelete?.(product.id))}>
            ❌ {$t('common.delete')}
          </button>
        </div>
      {/if}
    </div>

    <!-- Quick action bar (bottom of thumbnail, visible on hover) -->
    <div class="quick-actions">
      <button type="button" class="quick-btn" title={$t('products.openFolder')} onclick={handleOpenFolder}>
        <svg viewBox="0 0 20 20" fill="currentColor" width="14" height="14">
          <path d="M2 6a2 2 0 012-2h5l2 2h5a2 2 0 012 2v6a2 2 0 01-2 2H4a2 2 0 01-2-2V6z"/>
        </svg>
      </button>
      {#if dufPath}
        <button type="button" class="quick-btn accent" title="Open in DAZ Studio" onclick={handleOpenInDaz}>
          <svg viewBox="0 0 20 20" fill="currentColor" width="14" height="14">
            <path fill-rule="evenodd" d="M10 18a8 8 0 100-16 8 8 0 000 16zM9.555 7.168A1 1 0 008 8v4a1 1 0 001.555.832l3-2a1 1 0 000-1.664l-3-2z" clip-rule="evenodd"/>
          </svg>
        </button>
      {/if}
    </div>
  </div>

  <!-- Card body -->
  <div class="card-body">
    <h4 class="card-title" title={product.name}>{product.name}</h4>

    {#if product.vendor}
      <span class="card-vendor">{product.vendor}</span>
    {/if}

    {#if tagInfo.visible.length > 0}
      <div class="card-tags">
        {#each tagInfo.visible as tag}
          <span class="tag">{tag}</span>
        {/each}
        {#if tagInfo.extra > 0}
          <span class="tag muted">+{tagInfo.extra}</span>
        {/if}
      </div>
    {/if}

    <div class="card-meta">
      {#if product.totalSize > 0}
        <span>{formatFileSize(product.totalSize)}</span>
      {/if}
      {#if product.filesCount > 0}
        <span>{product.filesCount} files</span>
      {/if}
    </div>
  </div>
</div>

<style>
  .card {
    position: relative;
    background: var(--bg-tertiary);
    border-radius: 12px;
    overflow: hidden;
    border: 2px solid transparent;
    transition: border-color 0.15s, transform 0.15s, box-shadow 0.15s;
    cursor: pointer;
    user-select: none;
  }

  .card:hover {
    border-color: var(--border-color);
    transform: translateY(-2px);
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.15);
  }

  .card.selected {
    border-color: var(--accent, #8b5cf6);
    box-shadow: 0 0 0 1px rgba(139, 92, 246, 0.4), 0 4px 12px rgba(139, 92, 246, 0.15);
  }

  .card.selected .card-thumb::after {
    content: '✓';
    position: absolute;
    top: 6px;
    right: 6px;
    z-index: 15;
    width: 22px;
    height: 22px;
    border-radius: 50%;
    background: var(--accent, #8b5cf6);
    color: white;
    font-size: 0.7rem;
    font-weight: 700;
    display: flex;
    align-items: center;
    justify-content: center;
    pointer-events: none;
  }

  .card.focused {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
    box-shadow: 0 0 0 4px var(--accent-glow, rgba(233, 69, 96, 0.25));
  }

  /* ---- Thumbnail ---- */
  .card-thumb {
    position: relative;
    width: 100%;
    aspect-ratio: 1 / 1;
    background: #000;
    overflow: hidden;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .card-thumb img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
  }

  .thumb-fallback {
    width: 100%;
    height: 100%;
    display: flex;
    align-items: center;
    justify-content: center;
    background: linear-gradient(135deg, var(--bg-tertiary) 0%, var(--bg-secondary) 100%);
  }

  .thumb-icon-text {
    font-size: 1.6rem;
    font-weight: 800;
    color: var(--text-secondary);
    opacity: 0.4;
    letter-spacing: 2px;
    user-select: none;
  }

  /* ---- Overlay badges ---- */
  .badges-overlay {
    position: absolute;
    top: 6px;
    left: 6px;
    z-index: 10;
    display: flex;
    flex-direction: column;
    gap: 4px;
    max-width: calc(100% - 40px);
  }

  .badge {
    padding: 2px 7px;
    border-radius: 6px;
    font-size: 0.68rem;
    font-weight: 600;
    backdrop-filter: blur(6px);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 100%;
  }

  .type-badge {
    background: rgba(79, 70, 229, 0.8);
    color: white;
  }

  .lib-badge {
    background: rgba(0, 0, 0, 0.55);
    color: rgba(255, 255, 255, 0.9);
  }

  /* ---- Hover actions ---- */
  .hover-actions {
    position: absolute;
    top: 6px;
    right: 6px;
    opacity: 0;
    transition: opacity 0.15s;
  }

  .card:hover .hover-actions {
    opacity: 1;
  }

  /* ---- Quick action bar (bottom of thumbnail) ---- */
  .quick-actions {
    position: absolute;
    bottom: 0;
    left: 0;
    right: 0;
    display: flex;
    justify-content: center;
    gap: 6px;
    padding: 6px;
    background: linear-gradient(transparent, rgba(0, 0, 0, 0.6));
    opacity: 0;
    transition: opacity 0.15s;
  }

  .card:hover .quick-actions {
    opacity: 1;
  }

  .quick-btn {
    width: 30px;
    height: 30px;
    border-radius: 8px;
    border: none;
    background: rgba(255, 255, 255, 0.15);
    color: white;
    cursor: pointer;
    display: grid;
    place-items: center;
    backdrop-filter: blur(6px);
    transition: background 0.12s;
  }

  .quick-btn:hover {
    background: rgba(255, 255, 255, 0.3);
  }

  .quick-btn.accent {
    background: rgba(79, 70, 229, 0.7);
  }

  .quick-btn.accent:hover {
    background: rgba(79, 70, 229, 0.9);
  }

  .menu-btn {
    width: 28px;
    height: 28px;
    border-radius: 8px;
    border: none;
    background: rgba(0, 0, 0, 0.6);
    color: white;
    font-size: 1.1rem;
    cursor: pointer;
    display: grid;
    place-items: center;
    backdrop-filter: blur(6px);
  }

  .menu-btn:hover {
    background: rgba(0, 0, 0, 0.8);
  }

  .dropdown {
    position: absolute;
    top: 32px;
    right: 0;
    background: var(--bg-secondary);
    border: 1px solid var(--border-color);
    border-radius: 10px;
    padding: 4px;
    min-width: 150px;
    z-index: 20;
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.2);
  }

  .dropdown button {
    display: block;
    width: 100%;
    text-align: left;
    padding: 0.4rem 0.6rem;
    border: none;
    background: transparent;
    color: var(--text-primary);
    font-size: 0.82rem;
    cursor: pointer;
    border-radius: 6px;
    white-space: nowrap;
  }

  .dropdown button:hover {
    background: var(--bg-hover);
  }

  .dropdown button.danger:hover {
    background: rgba(239, 68, 68, 0.15);
    color: #ef4444;
  }

  /* ---- Card body ---- */
  .card-body {
    padding: 0.6rem 0.7rem 0.7rem;
    display: flex;
    flex-direction: column;
    gap: 0.3rem;
  }

  .card-title {
    margin: 0;
    font-size: 0.85rem;
    font-weight: 600;
    color: var(--text-primary);
    display: -webkit-box;
    -webkit-line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
    line-height: 1.3;
  }

  .card-vendor {
    font-size: 0.75rem;
    color: var(--text-secondary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .card-tags {
    display: flex;
    gap: 0.25rem;
    flex-wrap: wrap;
    margin-top: 0.1rem;
  }

  .tag {
    background: var(--accent);
    color: white;
    padding: 1px 6px;
    border-radius: 999px;
    font-size: 0.65rem;
  }

  .tag.muted {
    background: var(--bg-secondary);
    color: var(--text-secondary);
  }

  .card-meta {
    display: flex;
    gap: 0.6rem;
    color: var(--text-secondary);
    font-size: 0.72rem;
    margin-top: 0.15rem;
  }
</style>
