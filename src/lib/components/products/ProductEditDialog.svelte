<script lang="ts">
  import { onMount } from 'svelte';
  import { t } from '$lib/i18n';
  import { open } from '@tauri-apps/plugin-shell';
  import { convertFileSrc } from '@tauri-apps/api/core';
  import {
    formatDate,
    formatFileSize,
    parseTags,
    updateProduct,
    checkProductIntegrity,
    type Product,
    type IntegrityReport,
  } from '$lib/api/commands';
  import { KNOWN_CONTENT_TYPES, normalizeContentType, getContentTypeIcon } from './utils';
  import CustomSelect from '$lib/components/ui/CustomSelect.svelte';

  interface Props {
    product: Product;
    onclose?: () => void;
    onsaved?: (product: Product) => void;
    onuninstall?: (product: Product) => void;
  }

  let { product, onclose, onsaved, onuninstall }: Props = $props();

  let name = $state(product.name);
  let contentType = $state(normalizeContentType(product.contentType) ?? 'other');
  let tagsText = $state(product.tags ?? '');
  let notes = $state(product.notes ?? '');
  let saving = $state(false);
  let error = $state<string | null>(null);
  let copiedField = $state<string | null>(null);
  let dialogEl: HTMLDivElement | undefined = $state();

  // Integrity check state
  let integrityReport = $state<IntegrityReport | null>(null);
  let checkingIntegrity = $state(false);

  $effect(() => {
    name = product.name;
    contentType = normalizeContentType(product.contentType) ?? 'other';
    tagsText = product.tags ?? '';
    notes = product.notes ?? '';
    error = null;
  });

  onMount(() => {
    const firstInput = dialogEl?.querySelector<HTMLElement>('input, select, textarea, button');
    firstInput?.focus();
  });

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.preventDefault();
      onclose?.();
      return;
    }
    if (e.key === 'Tab' && dialogEl) {
      const focusable = dialogEl.querySelectorAll<HTMLElement>(
        'input, select, textarea, button:not(:disabled), [tabindex]:not([tabindex="-1"])',
      );
      if (focusable.length === 0) return;
      const first = focusable[0];
      const last = focusable[focusable.length - 1];
      if (e.shiftKey && document.activeElement === first) {
        e.preventDefault();
        last.focus();
      } else if (!e.shiftKey && document.activeElement === last) {
        e.preventDefault();
        first.focus();
      }
    }
  }

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

  async function handleIntegrityCheck() {
    if (checkingIntegrity) return;
    checkingIntegrity = true;
    try {
      integrityReport = await checkProductIntegrity(product.id);
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      checkingIntegrity = false;
    }
  }

  let canSave = $derived(name.trim().length > 0 && !saving);

  let thumbnailSrc = $derived.by(() =>
    product.thumbnailPath ? convertFileSrc(product.thumbnailPath) : null,
  );

  async function openInExplorer(path: string): Promise<void> {
    try {
      await open(path);
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
  }

  async function copyToClipboard(text: string, field: string): Promise<void> {
    try {
      await navigator.clipboard.writeText(text);
      copiedField = field;
      setTimeout(() => {
        if (copiedField === field) copiedField = null;
      }, 1500);
    } catch {
      // Fallback: do nothing
    }
  }

  interface MetaRow {
    label: string;
    value: string;
    copyable?: boolean;
    field: string;
  }

  let metaRows = $derived.by((): MetaRow[] => {
    const rows: MetaRow[] = [];
    rows.push({ label: $t('products.origin'), value: getOriginLabel(product.origin), field: 'origin' });
    if (product.libraryPath) {
      rows.push({ label: $t('products.libraryPath'), value: product.libraryPath, copyable: true, field: 'libraryPath' });
    }
    if (product.supportFile) {
      rows.push({ label: $t('products.supportFile'), value: product.supportFile, copyable: true, field: 'supportFile' });
    }
    if (product.vendor) {
      rows.push({ label: $t('products.vendor'), value: product.vendor, field: 'vendor' });
    }
    if (product.productToken) {
      rows.push({ label: $t('products.productToken'), value: product.productToken, copyable: true, field: 'productToken' });
    }
    if (product.globalId) {
      rows.push({ label: $t('products.globalId'), value: product.globalId, copyable: true, field: 'globalId' });
    }
    rows.push({
      label: $t('products.sourceArchive'),
      value: product.sourceArchive ?? (product.origin === 'library' ? $t('products.scanSource') : $t('common.unknown')),
      copyable: !!product.sourceArchive,
      field: 'sourceArchive',
    });
    rows.push({ label: $t('products.installDate'), value: formatDate(product.installedAt), field: 'installDate' });
    rows.push({ label: $t('products.filesCount'), value: String(product.filesCount), field: 'filesCount' });
    rows.push({ label: $t('products.totalSize'), value: formatFileSize(product.totalSize), field: 'totalSize' });
    return rows;
  });
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<!-- svelte-ignore a11y_interactive_supports_focus -->
<div class="overlay" role="dialog" aria-modal="true" aria-labelledby="dlg-title" bind:this={dialogEl} onkeydown={handleKeydown}>
  <div class="dialog">
    <!-- Header -->
    <header class="dlg-header">
      <h2 id="dlg-title">{$t('products.editProduct')}</h2>
      <button type="button" class="close-btn" onclick={() => onclose?.()} title={$t('common.close')}>
        <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
          <line x1="4" y1="4" x2="12" y2="12"/><line x1="12" y1="4" x2="4" y2="12"/>
        </svg>
      </button>
    </header>

    <!-- Body -->
    <div class="dlg-body">
      {#if error}
        <div class="error-banner">⚠️ {error}</div>
      {/if}

      <div class="split">
        <!-- Left column: editable fields -->
        <div class="col-edit">
          <!-- Thumbnail preview -->
          <div class="thumb-preview">
            {#if thumbnailSrc}
              <img src={thumbnailSrc} alt={product.name} />
            {:else}
              <div class="thumb-placeholder">
                <span>{getContentTypeIcon(product.contentType)}</span>
              </div>
            {/if}
          </div>

          <div class="form-group">
            <label for="ed-name">{$t('common.name')}</label>
            <input id="ed-name" type="text" bind:value={name} required />
          </div>

          <div class="form-group">
            <label for="ed-type">{$t('common.type')}</label>
            <CustomSelect
              id="ed-type"
              bind:value={contentType}
              options={KNOWN_CONTENT_TYPES.map(ct => ({ value: ct, label: getContentTypeLabel(ct) }))}
            />
          </div>

          <div class="form-group">
            <label for="ed-tags">{$t('products.tags')}</label>
            <input
              id="ed-tags"
              type="text"
              bind:value={tagsText}
              placeholder={$t('products.tagsPlaceholder')}
            />
          </div>

          <div class="form-group">
            <label for="ed-notes">{$t('products.notes')}</label>
            <textarea
              id="ed-notes"
              rows="4"
              bind:value={notes}
              placeholder={$t('products.notesPlaceholder')}
            ></textarea>
          </div>
        </div>

        <!-- Right column: read-only metadata -->
        <div class="col-meta">
          <h3 class="meta-title">{$t('products.metadata')}</h3>

          <!-- Path with open folder button -->
          <div class="meta-path-row">
            <span class="meta-label">{$t('common.path')}</span>
            <div class="meta-path-value">
              <code class="path-text" title={product.path}>{product.path}</code>
              <div class="path-actions">
                <button
                  type="button"
                  class="icon-action"
                  title={$t('products.openFolder')}
                  onclick={() => void openInExplorer(product.path)}
                >📁</button>
                <button
                  type="button"
                  class="icon-action"
                  class:copied={copiedField === 'path'}
                  title={copiedField === 'path' ? $t('common.copied') : $t('common.copy')}
                  onclick={() => void copyToClipboard(product.path, 'path')}
                >{copiedField === 'path' ? '✓' : '📋'}</button>
              </div>
            </div>
          </div>

          <!-- Categories -->
          {#if product.categories && product.categories.length > 0}
            <div class="meta-row">
              <span class="meta-label">{$t('products.categories')}</span>
              <div class="cat-chips">
                {#each product.categories as cat}
                  <span class="cat-chip">{cat}</span>
                {/each}
              </div>
            </div>
          {/if}

          <!-- Metadata table -->
          <div class="meta-table">
            {#each metaRows as row}
              <div class="meta-row">
                <span class="meta-label">{row.label}</span>
                <div class="meta-value-wrap">
                  <span class="meta-value" class:mono={row.copyable} title={row.value}>{row.value}</span>
                  {#if row.copyable}
                    <button
                      type="button"
                      class="copy-btn"
                      class:copied={copiedField === row.field}
                      title={copiedField === row.field ? $t('common.copied') : $t('common.copy')}
                      onclick={() => void copyToClipboard(row.value, row.field)}
                    >{copiedField === row.field ? '✓' : '📋'}</button>
                  {/if}
                </div>
              </div>
            {/each}
          </div>

          <!-- Integrity Check -->
          <div class="integrity-section">
            <button type="button" class="btn-integrity" onclick={handleIntegrityCheck} disabled={checkingIntegrity}>
              {#if checkingIntegrity}
                <span class="spinner-sm"></span>
              {:else}
                🔍
              {/if}
              Check Integrity
            </button>

            {#if integrityReport}
              <div class="integrity-result" class:ok={integrityReport.integrityPct === 100} class:warn={integrityReport.integrityPct < 100}>
                <div class="integrity-bar-wrap">
                  <div class="integrity-bar" style="width: {integrityReport.integrityPct}%"></div>
                </div>
                <span class="integrity-pct">{integrityReport.integrityPct.toFixed(0)}%</span>
                <span class="integrity-detail">
                  {integrityReport.filesPresent}/{integrityReport.totalFiles} files
                </span>
              </div>

              {#if integrityReport.missingPaths.length > 0}
                <details class="missing-files">
                  <summary>⚠️ {integrityReport.filesMissing} missing file{integrityReport.filesMissing > 1 ? 's' : ''}</summary>
                  <ul>
                    {#each integrityReport.missingPaths.slice(0, 50) as path}
                      <li>{path}</li>
                    {/each}
                    {#if integrityReport.missingPaths.length > 50}
                      <li class="truncated">… and {integrityReport.missingPaths.length - 50} more</li>
                    {/if}
                  </ul>
                </details>
              {/if}
            {/if}
          </div>
        </div>
      </div>
    </div>

    <!-- Footer -->
    <footer class="dlg-footer">
      <div class="footer-left">
        <button type="button" class="btn-uninstall" onclick={() => onuninstall?.(product)}>
          🗑️ Uninstall
        </button>
      </div>
      <div class="footer-right">
        <button type="button" class="btn-cancel" onclick={() => onclose?.()} disabled={saving}>
          {$t('common.cancel')}
        </button>
        <button type="button" class="btn-save" onclick={handleSave} disabled={!canSave}>
          {#if saving}
            <span class="spinner-sm"></span>
          {/if}
          {$t('common.save')}
        </button>
      </div>
    </footer>
  </div>
</div>

<style>
  /* ---- Overlay ---- */
  .overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.7);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1000;
    backdrop-filter: blur(4px);
  }

  .dialog {
    background: var(--bg-primary, #0f0f23);
    border: 1px solid var(--border-color, #333);
    border-radius: 14px;
    width: 94%;
    max-width: 860px;
    max-height: 85vh;
    display: flex;
    flex-direction: column;
    box-shadow: 0 24px 64px rgba(0, 0, 0, 0.5);
  }

  /* ---- Header ---- */
  .dlg-header {
    padding: 1rem 1.25rem;
    border-bottom: 1px solid var(--border-color, #333);
    display: flex;
    justify-content: space-between;
    align-items: center;
    flex-shrink: 0;
  }

  .dlg-header h2 {
    margin: 0;
    font-size: 1.1rem;
    font-weight: 600;
    color: var(--text-primary);
  }

  .close-btn {
    width: 32px;
    height: 32px;
    border-radius: 8px;
    border: none;
    background: transparent;
    color: var(--text-secondary);
    cursor: pointer;
    display: grid;
    place-items: center;
  }

  .close-btn:hover {
    background: var(--bg-hover);
    color: var(--text-primary);
  }

  /* ---- Body ---- */
  .dlg-body {
    padding: 1.25rem;
    overflow-y: auto;
    flex: 1;
    min-height: 0;
  }

  .error-banner {
    background: rgba(239, 68, 68, 0.1);
    border: 1px solid var(--error);
    border-radius: 10px;
    padding: 0.6rem 0.9rem;
    color: var(--error);
    font-size: 0.85rem;
    margin-bottom: 1rem;
  }

  /* ---- Split layout ---- */
  .split {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 1.5rem;
  }

  /* ---- Left: Edit column ---- */
  .col-edit {
    display: flex;
    flex-direction: column;
    gap: 0.85rem;
  }

  .thumb-preview {
    width: 100%;
    aspect-ratio: 4 / 3;
    border-radius: 10px;
    overflow: hidden;
    background: var(--bg-secondary);
    border: 1px solid var(--border-color);
  }

  .thumb-preview img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .thumb-placeholder {
    width: 100%;
    height: 100%;
    display: flex;
    align-items: center;
    justify-content: center;
    background: linear-gradient(135deg, var(--bg-tertiary), var(--bg-secondary));
  }

  .thumb-placeholder span {
    font-size: 2rem;
    font-weight: 800;
    color: var(--text-secondary);
    opacity: 0.3;
    letter-spacing: 3px;
  }

  .form-group {
    display: flex;
    flex-direction: column;
    gap: 0.3rem;
  }

  .form-group label {
    font-size: 0.8rem;
    font-weight: 500;
    color: var(--text-secondary);
    text-transform: uppercase;
    letter-spacing: 0.4px;
  }

  .form-group input,
  .form-group select,
  .form-group textarea {
    padding: 0.5rem 0.65rem;
    border: 1px solid var(--border-color);
    border-radius: 8px;
    background: var(--bg-secondary);
    color: var(--text-primary);
    font-family: inherit;
    font-size: 0.9rem;
    transition: border-color 0.15s;
  }

  .form-group input:focus,
  .form-group select:focus,
  .form-group textarea:focus {
    outline: none;
    border-color: var(--accent);
  }

  .form-group textarea {
    resize: vertical;
    min-height: 80px;
  }

  /* ---- Right: Metadata column ---- */
  .col-meta {
    display: flex;
    flex-direction: column;
    gap: 0.6rem;
  }

  .meta-title {
    margin: 0;
    font-size: 0.85rem;
    font-weight: 600;
    color: var(--text-secondary);
    text-transform: uppercase;
    letter-spacing: 0.5px;
    padding-bottom: 0.4rem;
    border-bottom: 1px solid var(--border-color);
  }

  /* ---- Path row (special) ---- */
  .meta-path-row {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
  }

  .meta-path-value {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    min-width: 0;
  }

  .path-text {
    flex: 1;
    font-family: monospace;
    font-size: 0.78rem;
    color: var(--text-primary);
    background: var(--bg-secondary);
    padding: 0.35rem 0.5rem;
    border-radius: 6px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
  }

  .path-actions {
    display: flex;
    gap: 0.2rem;
    flex-shrink: 0;
  }

  .icon-action {
    width: 28px;
    height: 28px;
    border-radius: 6px;
    border: 1px solid var(--border-color);
    background: var(--bg-secondary);
    cursor: pointer;
    display: grid;
    place-items: center;
    font-size: 0.75rem;
    transition: background 0.1s;
  }

  .icon-action:hover {
    background: var(--bg-hover);
  }

  .icon-action.copied {
    background: rgba(34, 197, 94, 0.15);
    border-color: rgba(34, 197, 94, 0.4);
    color: #22c55e;
  }

  /* ---- Category chips ---- */
  .cat-chips {
    display: flex;
    flex-wrap: wrap;
    gap: 0.3rem;
  }

  .cat-chip {
    background: rgba(79, 70, 229, 0.12);
    color: var(--accent);
    border-radius: 6px;
    padding: 0.2rem 0.5rem;
    font-size: 0.72rem;
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* ---- Meta table ---- */
  .meta-table {
    display: flex;
    flex-direction: column;
    gap: 0.1rem;
  }

  .meta-row {
    display: flex;
    flex-direction: column;
    gap: 0.15rem;
    padding: 0.4rem 0;
    border-bottom: 1px solid rgba(128, 128, 128, 0.1);
  }

  .meta-row:last-child {
    border-bottom: none;
  }

  .meta-label {
    font-size: 0.75rem;
    color: var(--text-secondary);
    font-weight: 500;
  }

  .meta-value-wrap {
    display: flex;
    align-items: center;
    gap: 0.35rem;
    min-width: 0;
  }

  .meta-value {
    font-size: 0.85rem;
    color: var(--text-primary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
    flex: 1;
  }

  .meta-value.mono {
    font-family: monospace;
    font-size: 0.78rem;
  }

  .copy-btn {
    width: 24px;
    height: 24px;
    border-radius: 5px;
    border: 1px solid transparent;
    background: transparent;
    cursor: pointer;
    display: grid;
    place-items: center;
    font-size: 0.7rem;
    color: var(--text-secondary);
    flex-shrink: 0;
    opacity: 0.5;
    transition: opacity 0.1s, background 0.1s;
  }

  .copy-btn:hover {
    opacity: 1;
    background: var(--bg-hover);
    border-color: var(--border-color);
  }

  .copy-btn.copied {
    opacity: 1;
    color: #22c55e;
    background: rgba(34, 197, 94, 0.1);
  }

  /* ---- Footer ---- */
  .dlg-footer {
    padding: 0.85rem 1.25rem;
    border-top: 1px solid var(--border-color, #333);
    display: flex;
    gap: 0.6rem;
    justify-content: space-between;
    align-items: center;
    flex-shrink: 0;
  }

  .footer-left, .footer-right {
    display: flex;
    gap: 0.6rem;
    align-items: center;
  }

  .btn-cancel,
  .btn-save {
    padding: 0.5rem 1rem;
    border-radius: 8px;
    cursor: pointer;
    font-size: 0.85rem;
    font-weight: 500;
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
  }

  .btn-cancel {
    background: var(--bg-tertiary);
    color: var(--text-primary);
    border: 1px solid var(--border-color);
  }

  .btn-cancel:hover {
    background: var(--bg-hover);
  }

  .btn-save {
    background: var(--accent);
    color: white;
    border: none;
  }

  .btn-save:hover {
    filter: brightness(1.1);
  }

  .btn-cancel:disabled,
  .btn-save:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  .spinner-sm {
    width: 14px;
    height: 14px;
    border: 2px solid rgba(255, 255, 255, 0.3);
    border-top-color: white;
    border-radius: 50%;
    display: inline-block;
    animation: spin 0.8s linear infinite;
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }

  /* ---- Responsive ---- */
  @media (max-width: 700px) {
    .split {
      grid-template-columns: 1fr;
    }

    .dialog {
      max-width: 100%;
      width: 100%;
      max-height: 95vh;
      border-radius: 14px 14px 0 0;
    }

    .thumb-preview {
      aspect-ratio: 16 / 9;
    }
  }

  /* ---- Uninstall button ---- */
  .btn-uninstall {
    padding: 0.5rem 1rem;
    border-radius: 8px;
    cursor: pointer;
    font-size: 0.85rem;
    font-weight: 500;
    background: rgba(239, 68, 68, 0.1);
    color: #ef4444;
    border: 1px solid rgba(239, 68, 68, 0.3);
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
    transition: all 0.15s;
  }

  .btn-uninstall:hover {
    background: rgba(239, 68, 68, 0.2);
    border-color: #ef4444;
  }

  /* ---- Integrity section ---- */
  .integrity-section {
    margin-top: 0.75rem;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  .btn-integrity {
    padding: 0.4rem 0.75rem;
    border-radius: 6px;
    cursor: pointer;
    font-size: 0.8rem;
    background: rgba(255, 255, 255, 0.06);
    color: var(--text-secondary, #888);
    border: 1px solid var(--border-color, #333);
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
    align-self: flex-start;
    transition: all 0.15s;
  }

  .btn-integrity:hover {
    background: rgba(255, 255, 255, 0.1);
    color: var(--text-primary, #fff);
  }

  .btn-integrity:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  .integrity-result {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    font-size: 0.8rem;
  }

  .integrity-bar-wrap {
    flex: 1;
    height: 6px;
    background: rgba(255, 255, 255, 0.08);
    border-radius: 3px;
    overflow: hidden;
    min-width: 60px;
    max-width: 120px;
  }

  .integrity-bar {
    height: 100%;
    border-radius: 3px;
    transition: width 0.3s ease;
  }

  .integrity-result.ok .integrity-bar {
    background: #22c55e;
  }

  .integrity-result.warn .integrity-bar {
    background: #f59e0b;
  }

  .integrity-pct {
    font-weight: 600;
    min-width: 3ch;
  }

  .integrity-result.ok .integrity-pct {
    color: #22c55e;
  }

  .integrity-result.warn .integrity-pct {
    color: #f59e0b;
  }

  .integrity-detail {
    color: var(--text-secondary, #888);
  }

  .missing-files {
    font-size: 0.75rem;
    color: var(--text-secondary, #888);
    margin-top: 0.25rem;
  }

  .missing-files summary {
    cursor: pointer;
    color: #f59e0b;
    font-size: 0.8rem;
  }

  .missing-files ul {
    list-style: none;
    padding: 0.25rem 0;
    margin: 0.25rem 0 0 0;
    max-height: 120px;
    overflow-y: auto;
  }

  .missing-files li {
    padding: 2px 0;
    color: #ef4444;
    font-family: monospace;
    font-size: 0.7rem;
    word-break: break-all;
  }

  .missing-files li.truncated {
    color: var(--text-secondary, #888);
    font-style: italic;
    font-family: inherit;
  }
</style>
