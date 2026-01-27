<script lang="ts">
  import { t } from '$lib/i18n';
  import { formatDate, formatFileSize, parseTags, type Product } from '$lib/api/commands';
  import { open } from '@tauri-apps/plugin-shell';
  import { convertFileSrc } from '@tauri-apps/api/core';
  import { getContentTypeIcon, normalizeContentType } from './utils';

  interface Props {
    product: Product;
    libraryName?: string | null;
    ondelete?: (id: number) => void;
    onedit?: (product: Product) => void;
  }

  let { product, libraryName = null, ondelete, onedit }: Props = $props();

  function getContentTypeLabel(type: string | null): string {
    const normalized = normalizeContentType(type);
    if (!normalized) return $t('common.unknown');
    const key = `products.contentTypes.${normalized}`;
    const translated = $t(key);
    return translated !== key ? translated : normalized;
  }

  function normalizeCategoryPath(value: string): string {
    return value.replace(/\\/g, '/').replace(/^\/+|\/+$/g, '');
  }

  function getCategoryLabel(categories: string[]): string | null {
    const normalized = categories
      .map((value) => normalizeCategoryPath(value))
      .filter((value) => value.length > 0);

    if (normalized.length === 0) return null;

    normalized.sort((a, b) => a.split('/').length - b.split('/').length);
    const primary = normalized[0];
    const extra = normalized.length - 1;

    if (extra > 0) return `${primary} (+${extra})`;
    return primary;
  }

  let thumbnailSrc = $derived.by(() =>
    product.thumbnailPath ? convertFileSrc(product.thumbnailPath) : null
  );

  let subtitle = $derived.by(() => {
    const parts: string[] = [];
    if (product.vendor) parts.push(product.vendor);
    const typeLabel = getContentTypeLabel(product.contentType);
    if (typeLabel) parts.push(typeLabel);
    return parts.join(' | ');
  });

  let categoryLabel = $derived.by(() => getCategoryLabel(product.categories ?? []));

  let tagInfo = $derived.by(() => {
    const tags = parseTags(product.tags ?? '');
    const visible = tags.slice(0, 4);
    return { visible, extra: Math.max(0, tags.length - visible.length) };
  });

  async function openInExplorer(path: string): Promise<void> {
    try {
      await open(path);
    } catch (e) {
      console.warn('[Products] Failed to open path:', path, e);
    }
  }
</script>

<div class="product-card">
  <div class="thumb">
    {#if thumbnailSrc}
      <img src={thumbnailSrc} alt={product.name} loading="lazy" />
    {:else}
      <span class="thumb-placeholder">{getContentTypeIcon(product.contentType)}</span>
    {/if}
  </div>

  <div class="product-body">
    <div class="product-header">
      <div class="product-title">
        <span class="product-name">{product.name}</span>
        {#if subtitle}
          <span class="product-subtitle">{subtitle}</span>
        {/if}
      </div>
      <div class="actions">
        <button
          type="button"
          class="icon-btn"
          title={$t('products.openFolder')}
          onclick={() => void openInExplorer(product.path)}
        >
          {$t('products.openFolder')}
        </button>
        <button
          type="button"
          class="icon-btn"
          title={$t('common.edit')}
          onclick={() => onedit?.(product)}
        >
          {$t('common.edit')}
        </button>
        <button
          type="button"
          class="icon-btn danger"
          title={$t('common.delete')}
          onclick={() => ondelete?.(product.id)}
        >
          {$t('common.delete')}
        </button>
      </div>
    </div>

    <div class="badges">
      {#if libraryName}
        <span class="badge">{libraryName}</span>
      {/if}
      {#if categoryLabel}
        <span class="badge category">{categoryLabel}</span>
      {/if}
    </div>

    <div class="product-path" title={product.path}>{product.path}</div>

    {#if tagInfo.visible.length > 0}
      <div class="product-tags">
        {#each tagInfo.visible as tag}
          <span class="tag">{tag}</span>
        {/each}
        {#if tagInfo.extra > 0}
          <span class="tag muted">+{tagInfo.extra}</span>
        {/if}
      </div>
    {/if}

    <div class="product-meta">
      <span title={$t('products.installDate')}>{formatDate(product.installedAt)}</span>
      {#if product.filesCount > 0}
        <span title={$t('products.filesCount')}>{product.filesCount}</span>
      {/if}
      {#if product.totalSize > 0}
        <span title={$t('products.totalSize')}>{formatFileSize(product.totalSize)}</span>
      {/if}
    </div>
  </div>
</div>

<style>
  .product-card {
    background-color: var(--bg-tertiary);
    border-radius: var(--border-radius);
    padding: 0.9rem;
    display: grid;
    grid-template-columns: 92px 1fr;
    gap: 0.9rem;
    border: 1px solid transparent;
  }

  .product-card:hover {
    border-color: var(--border-color);
  }

  .thumb {
    width: 92px;
    height: 92px;
    border-radius: 12px;
    overflow: hidden;
    background: var(--bg-secondary);
    display: grid;
    place-items: center;
    border: 1px solid var(--border-color);
  }

  .thumb img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .thumb-placeholder {
    font-size: 0.8rem;
    font-weight: 700;
    color: var(--text-secondary);
  }

  .product-body {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    min-width: 0;
  }

  .product-header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 0.75rem;
  }

  .product-title {
    display: flex;
    flex-direction: column;
    min-width: 0;
    gap: 0.2rem;
  }

  .product-name {
    font-weight: 700;
    color: var(--text-primary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .product-subtitle {
    font-size: 0.82rem;
    color: var(--text-secondary);
  }

  .actions {
    display: flex;
    gap: 0.35rem;
    flex-wrap: wrap;
    justify-content: flex-end;
  }

  .icon-btn {
    background: transparent;
    border: 1px solid var(--border-color);
    border-radius: 8px;
    padding: 0.3rem 0.45rem;
    cursor: pointer;
    line-height: 1.1;
    font-size: 0.75rem;
    color: var(--text-primary);
  }

  .icon-btn:hover {
    background: var(--bg-hover);
  }

  .icon-btn.danger:hover {
    background: rgba(239, 68, 68, 0.15);
    border-color: rgba(239, 68, 68, 0.35);
  }

  .badges {
    display: flex;
    gap: 0.4rem;
    flex-wrap: wrap;
  }

  .badge {
    background: var(--bg-secondary);
    color: var(--text-secondary);
    border-radius: 999px;
    padding: 0.2rem 0.55rem;
    font-size: 0.72rem;
  }

  .badge.category {
    background: rgba(79, 70, 229, 0.15);
    color: var(--accent);
  }

  .product-path {
    font-family: monospace;
    font-size: 0.78rem;
    color: var(--text-secondary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .product-tags {
    display: flex;
    gap: 0.35rem;
    flex-wrap: wrap;
  }

  .tag {
    background-color: var(--accent);
    color: white;
    padding: 0.1rem 0.45rem;
    border-radius: 999px;
    font-size: 0.72rem;
  }

  .tag.muted {
    background: var(--bg-secondary);
    color: var(--text-secondary);
  }

  .product-meta {
    display: flex;
    gap: 0.8rem;
    flex-wrap: wrap;
    color: var(--text-secondary);
    font-size: 0.82rem;
  }

  @media (max-width: 640px) {
    .product-card {
      grid-template-columns: 1fr;
    }

    .thumb {
      width: 100%;
      height: 160px;
    }
  }
</style>
