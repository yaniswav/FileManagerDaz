<script lang="ts">
  import { t } from '$lib/i18n';
  import { open } from '@tauri-apps/plugin-shell';
  import {
    formatDate,
    formatFileSize,
    parseTags,
    updateProduct,
    type Product,
  } from '$lib/api/commands';
  import { KNOWN_CONTENT_TYPES, normalizeContentType } from './utils';

  interface Props {
    product: Product;
    onclose?: () => void;
    onsaved?: (product: Product) => void;
  }

  let { product, onclose, onsaved }: Props = $props();

  let name = $state(product.name);
  let contentType = $state(normalizeContentType(product.contentType) ?? 'other');
  let tagsText = $state(product.tags ?? '');
  let notes = $state(product.notes ?? '');
  let saving = $state(false);
  let error = $state<string | null>(null);

  $effect(() => {
    name = product.name;
    contentType = normalizeContentType(product.contentType) ?? 'other';
    tagsText = product.tags ?? '';
    notes = product.notes ?? '';
    error = null;
  });

  function getContentTypeLabel(type: string): string {
    const normalized = normalizeContentType(type);
    if (!normalized) return $t('common.unknown');
    const key = `products.contentTypes.${normalized}`;
    const translated = $t(key);
    return translated !== key ? translated : normalized;
  }

  function getOriginLabel(origin?: string | null): string {
    if (origin === 'library') return $t('products.originLibrary');
    if (origin === 'import') return $t('products.originImport');
    return $t('common.unknown');
  }

  async function handleSave() {
    if (saving) return;
    if (name.trim().length === 0) return;
    saving = true;
    error = null;

    try {
      const trimmedName = name.trim();
      const tags = parseTags(tagsText.replace(/\n/g, ','));
      const normalized = normalizeContentType(contentType) ?? null;

      const updated = await updateProduct(product.id, {
        name: trimmedName,
        tags,
        contentType: normalized ?? 'other',
        notes,
      });

      onsaved?.(updated);
      onclose?.();
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      saving = false;
    }
  }

  let canSave = $derived(name.trim().length > 0 && !saving);

  async function openInExplorer(path: string): Promise<void> {
    try {
      await open(path);
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
  }
</script>

<div class="dialog-overlay" role="dialog" aria-modal="true" aria-labelledby="dialog-title">
  <div class="dialog-container">
    <header class="dialog-header">
      <h2 id="dialog-title">{$t('products.details')}</h2>
      <button type="button" class="icon-btn" onclick={() => onclose?.()} title={$t('common.close')}>
        X
      </button>
    </header>

    <div class="dialog-content">
      {#if error}
        <div class="error-banner">
          <span>{$t('common.error')}: {error}</span>
        </div>
      {/if}

      <div class="grid">
        <div class="field">
          <label for="product-name">{$t('common.name')}</label>
          <input id="product-name" type="text" bind:value={name} required />
        </div>

        <div class="field">
          <label for="product-type">{$t('common.type')}</label>
          <select id="product-type" bind:value={contentType}>
            {#each KNOWN_CONTENT_TYPES as ct}
              <option value={ct}>{getContentTypeLabel(ct)}</option>
            {/each}
          </select>
        </div>

        <div class="field full">
          <label for="product-tags">{$t('products.tags')}</label>
          <input
            id="product-tags"
            type="text"
            bind:value={tagsText}
            placeholder={$t('products.tagsPlaceholder')}
          />
        </div>

        <div class="field full">
          <label for="product-notes">{$t('products.notes')}</label>
          <textarea
            id="product-notes"
            rows="5"
            bind:value={notes}
            placeholder={$t('products.notesPlaceholder')}
          ></textarea>
        </div>
      </div>

      <div class="details">
        <div class="detail-row">
          <span class="detail-label">{$t('products.origin')}</span>
          <span>{getOriginLabel(product.origin)}</span>
        </div>
        {#if product.libraryPath}
          <div class="detail-row">
            <span class="detail-label">{$t('products.libraryPath')}</span>
            <code class="mono">{product.libraryPath}</code>
          </div>
        {/if}
        {#if product.supportFile}
          <div class="detail-row">
            <span class="detail-label">{$t('products.supportFile')}</span>
            <code class="mono">{product.supportFile}</code>
          </div>
        {/if}
        {#if product.vendor}
          <div class="detail-row">
            <span class="detail-label">{$t('products.vendor')}</span>
            <span>{product.vendor}</span>
          </div>
        {/if}
        {#if product.productToken}
          <div class="detail-row">
            <span class="detail-label">{$t('products.productToken')}</span>
            <span>{product.productToken}</span>
          </div>
        {/if}
        {#if product.globalId}
          <div class="detail-row">
            <span class="detail-label">{$t('products.globalId')}</span>
            <span>{product.globalId}</span>
          </div>
        {/if}
        {#if product.categories && product.categories.length > 0}
          <div class="detail-row">
            <span class="detail-label">{$t('products.categories')}</span>
            <div class="category-list">
              {#each product.categories as category}
                <span class="category-chip">{category}</span>
              {/each}
            </div>
          </div>
        {/if}
        <div class="detail-row">
          <span class="detail-label">{$t('common.path')}</span>
          <div class="detail-value">
            <code class="mono">{product.path}</code>
            <button
              type="button"
              class="small-btn"
              onclick={() => void openInExplorer(product.path)}
              title={$t('products.openFolder')}
            >
              {$t('products.openFolder')}
            </button>
          </div>
        </div>
        <div class="detail-row">
          <span class="detail-label">{$t('products.sourceArchive')}</span>
          <code class="mono">
            {product.sourceArchive ?? (product.origin === 'library' ? $t('products.scanSource') : $t('common.unknown'))}
          </code>
        </div>
        <div class="detail-row">
          <span class="detail-label">{$t('products.installDate')}</span>
          <span>{formatDate(product.installedAt)}</span>
        </div>
        <div class="detail-row">
          <span class="detail-label">{$t('products.filesCount')}</span>
          <span>{product.filesCount}</span>
        </div>
        <div class="detail-row">
          <span class="detail-label">{$t('products.totalSize')}</span>
          <span>{formatFileSize(product.totalSize)}</span>
        </div>
      </div>
    </div>

    <footer class="dialog-footer">
      <button type="button" class="btn-secondary" onclick={() => onclose?.()} disabled={saving}>
        {$t('common.cancel')}
      </button>
      <button type="button" class="btn-primary" onclick={handleSave} disabled={!canSave}>
        {#if saving}
          <span class="btn-spinner"></span> {$t('common.loading')}
        {:else}
          {$t('common.save')}
        {/if}
      </button>
    </footer>
  </div>
</div>

<style>
  .dialog-overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.7);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1000;
    backdrop-filter: blur(4px);
  }

  .dialog-container {
    background: var(--bg-primary, #0f0f23);
    border: 1px solid var(--border-color, #333);
    border-radius: 12px;
    width: 92%;
    max-width: 760px;
    max-height: 85vh;
    display: flex;
    flex-direction: column;
    box-shadow: 0 20px 60px rgba(0, 0, 0, 0.5);
  }

  .dialog-header {
    padding: 1.25rem 1.5rem;
    border-bottom: 1px solid var(--border-color, #333);
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 1rem;
  }

  .dialog-header h2 {
    margin: 0;
    font-size: 1.15rem;
    color: var(--text-primary, #fff);
  }

  .icon-btn {
    background: transparent;
    border: 1px solid transparent;
    border-radius: 8px;
    padding: 0.25rem 0.5rem;
    cursor: pointer;
    color: var(--text-primary, #fff);
  }

  .icon-btn:hover {
    background: var(--bg-hover);
    border-color: var(--border-color);
  }

  .dialog-content {
    padding: 1.25rem 1.5rem;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 1rem;
  }

  .error-banner {
    background: rgba(239, 68, 68, 0.1);
    border: 1px solid var(--error);
    border-radius: 10px;
    padding: 0.75rem 1rem;
    color: var(--error);
  }

  .grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 0.9rem 1rem;
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 0.35rem;
  }

  .field.full {
    grid-column: 1 / -1;
  }

  label {
    font-size: 0.85rem;
    color: var(--text-secondary);
  }

  input,
  select,
  textarea {
    padding: 0.55rem 0.7rem;
    border: 1px solid var(--border-color);
    border-radius: 10px;
    background: var(--bg-secondary);
    color: var(--text-primary);
    font-family: inherit;
  }

  textarea {
    resize: vertical;
  }

  .details {
    border-top: 1px solid var(--border-color);
    padding-top: 1rem;
    display: flex;
    flex-direction: column;
    gap: 0.6rem;
  }

  .detail-row {
    display: flex;
    gap: 0.75rem;
    align-items: baseline;
  }

  .detail-value {
    display: flex;
    gap: 0.75rem;
    flex: 1;
    min-width: 0;
    align-items: baseline;
  }

  .detail-label {
    width: 160px;
    flex-shrink: 0;
    color: var(--text-secondary);
    font-size: 0.85rem;
  }

  .mono {
    font-family: monospace;
    font-size: 0.82rem;
    word-break: break-all;
  }

  .category-list {
    display: flex;
    flex-wrap: wrap;
    gap: 0.4rem;
  }

  .category-chip {
    background: var(--bg-secondary);
    color: var(--text-secondary);
    border-radius: 999px;
    padding: 0.2rem 0.55rem;
    font-size: 0.72rem;
  }

  .small-btn {
    background: var(--bg-tertiary);
    color: var(--text-primary);
    border: 1px solid var(--border-color);
    border-radius: 10px;
    padding: 0.35rem 0.6rem;
    cursor: pointer;
    white-space: nowrap;
  }

  .small-btn:hover {
    background: var(--bg-hover);
  }

  .dialog-footer {
    padding: 1rem 1.5rem;
    border-top: 1px solid var(--border-color, #333);
    display: flex;
    gap: 0.75rem;
    justify-content: flex-end;
  }

  .btn-primary {
    background: var(--accent);
    color: white;
    border: none;
    padding: 0.55rem 1rem;
    border-radius: 10px;
    cursor: pointer;
  }

  .btn-secondary {
    background: var(--bg-tertiary);
    color: var(--text-primary);
    border: 1px solid var(--border-color);
    padding: 0.55rem 1rem;
    border-radius: 10px;
    cursor: pointer;
  }

  .btn-secondary:disabled,
  .btn-primary:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  .btn-spinner {
    width: 14px;
    height: 14px;
    border: 2px solid var(--border-color, #333);
    border-top-color: white;
    border-radius: 50%;
    display: inline-block;
    margin-right: 0.5rem;
    animation: spin 0.8s linear infinite;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  @media (max-width: 640px) {
    .grid {
      grid-template-columns: 1fr;
    }

    .detail-label {
      width: 120px;
    }
  }
</style>
