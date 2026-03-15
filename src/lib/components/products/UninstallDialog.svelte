<script lang="ts">
  import { uninstallProduct, formatFileSize, type UninstallReport, type Product } from '$lib/api/commands';
  import { addToast } from '$lib/stores/toast.svelte';
  import { t } from '$lib/i18n';

  interface Props {
    product: Product;
    onclose: () => void;
    onuninstalled: (productId: number) => void;
  }

  let { product, onclose, onuninstalled }: Props = $props();

  let phase = $state<'loading' | 'preview' | 'deleting' | 'done' | 'error'>('loading');
  let report = $state<UninstallReport | null>(null);
  let errorMsg = $state('');

  // Load dry-run preview on mount
  $effect(() => {
    loadPreview();
  });

  async function loadPreview() {
    try {
      phase = 'loading';
      report = await uninstallProduct(product.id, true);
      phase = 'preview';
    } catch (e: any) {
      errorMsg = e?.message ?? String(e);
      phase = 'error';
    }
  }

  async function confirmUninstall() {
    try {
      phase = 'deleting';
      const result = await uninstallProduct(product.id, false);
      report = result;
      phase = 'done';

      if (result.errors.length > 0) {
        addToast(
          `Uninstalled "${product.name}" with ${result.errors.length} warning(s)`,
          'info'
        );
      } else {
        addToast(
          `"${product.name}" uninstalled — ${formatFileSize(result.bytesFreed)} freed`,
          'success'
        );
      }

      onuninstalled(product.id);
      onclose();
    } catch (e: any) {
      errorMsg = e?.message ?? String(e);
      phase = 'error';
    }
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<!-- svelte-ignore a11y_click_events_have_key_events -->
<div class="overlay" onclick={onclose}>
  <div class="dialog danger" onclick={(e) => e.stopPropagation()}>
    <header>
      <h3>🗑️ Uninstall Product</h3>
      <button class="close-btn" onclick={onclose}>✕</button>
    </header>

    <div class="body">
      <p class="product-name">{product.name}</p>

      {#if phase === 'loading'}
        <div class="loading">
          <span class="spinner"></span>
          <p>Scanning files…</p>
        </div>
      {:else if phase === 'error'}
        <div class="error-box">
          <p>⚠️ {errorMsg}</p>
          <button class="btn secondary" onclick={loadPreview}>Retry</button>
        </div>
      {:else if phase === 'preview' && report}
        <div class="preview">
          <div class="stat-row">
            <span class="label">Files to delete:</span>
            <span class="value">{report.filesFound}</span>
          </div>
          <div class="stat-row">
            <span class="label">Already missing:</span>
            <span class="value muted">{report.filesMissing}</span>
          </div>
          <div class="stat-row highlight">
            <span class="label">Disk space freed:</span>
            <span class="value">{formatFileSize(report.bytesFreed)}</span>
          </div>
        </div>

        {#if report.filesFound === 0 && report.filesMissing === 0}
          <p class="note">No tracked files found. Only the database entry will be removed.</p>
        {/if}

        <div class="warning-box">
          <p>⚠️ This action is <strong>permanent</strong>. Files will be deleted from your disk.</p>
        </div>
      {:else if phase === 'deleting'}
        <div class="loading">
          <span class="spinner"></span>
          <p>Deleting files…</p>
        </div>
      {/if}
    </div>

    <footer>
      <button class="btn secondary" onclick={onclose} disabled={phase === 'deleting'}>
        Cancel
      </button>
      {#if phase === 'preview'}
        <button class="btn danger" onclick={confirmUninstall}>
          🗑️ Uninstall permanently
        </button>
      {/if}
    </footer>
  </div>
</div>

<style>
  .overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.6);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 9999;
    backdrop-filter: blur(4px);
  }

  .dialog {
    background: var(--bg-primary, #1e1e2e);
    border-radius: 12px;
    width: min(440px, 90vw);
    box-shadow: 0 8px 32px rgba(0, 0, 0, 0.5);
    overflow: hidden;
  }

  .dialog.danger {
    border: 1px solid rgba(239, 68, 68, 0.3);
  }

  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 1rem 1.25rem;
    border-bottom: 1px solid rgba(255, 255, 255, 0.08);
  }

  header h3 {
    margin: 0;
    font-size: 1.1rem;
    color: #ef4444;
  }

  .close-btn {
    background: none;
    border: none;
    color: var(--text-secondary, #888);
    font-size: 1.2rem;
    cursor: pointer;
    padding: 4px;
    border-radius: 4px;
  }

  .close-btn:hover {
    background: rgba(255, 255, 255, 0.1);
  }

  .body {
    padding: 1.25rem;
  }

  .product-name {
    font-weight: 600;
    font-size: 1rem;
    margin: 0 0 1rem 0;
    color: var(--text-primary, #fff);
    word-break: break-word;
  }

  .loading {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 0.75rem;
    padding: 1.5rem 0;
    color: var(--text-secondary, #888);
  }

  .spinner {
    width: 28px;
    height: 28px;
    border: 3px solid rgba(255, 255, 255, 0.1);
    border-top-color: var(--accent, #7c3aed);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }

  .error-box {
    background: rgba(239, 68, 68, 0.1);
    border: 1px solid rgba(239, 68, 68, 0.3);
    border-radius: 8px;
    padding: 1rem;
    text-align: center;
    color: #ef4444;
  }

  .preview {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    margin-bottom: 1rem;
  }

  .stat-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 0.5rem 0.75rem;
    background: rgba(255, 255, 255, 0.03);
    border-radius: 6px;
  }

  .stat-row.highlight {
    background: rgba(239, 68, 68, 0.1);
  }

  .stat-row .label {
    color: var(--text-secondary, #888);
    font-size: 0.9rem;
  }

  .stat-row .value {
    font-weight: 600;
    color: var(--text-primary, #fff);
  }

  .stat-row .value.muted {
    color: var(--text-secondary, #888);
    font-weight: 400;
  }

  .warning-box {
    background: rgba(239, 68, 68, 0.08);
    border: 1px solid rgba(239, 68, 68, 0.2);
    border-radius: 8px;
    padding: 0.75rem 1rem;
    font-size: 0.85rem;
    color: #fca5a5;
    margin-top: 0.5rem;
  }

  .note {
    font-size: 0.85rem;
    color: var(--text-secondary, #888);
    font-style: italic;
    margin: 0 0 0.5rem;
  }

  footer {
    display: flex;
    justify-content: flex-end;
    gap: 0.75rem;
    padding: 1rem 1.25rem;
    border-top: 1px solid rgba(255, 255, 255, 0.08);
  }

  .btn {
    padding: 0.5rem 1.25rem;
    border-radius: 8px;
    font-size: 0.9rem;
    font-weight: 500;
    cursor: pointer;
    border: none;
    transition: all 0.15s;
  }

  .btn.secondary {
    background: rgba(255, 255, 255, 0.08);
    color: var(--text-primary, #fff);
  }

  .btn.secondary:hover {
    background: rgba(255, 255, 255, 0.12);
  }

  .btn.danger {
    background: #ef4444;
    color: #fff;
  }

  .btn.danger:hover {
    background: #dc2626;
  }

  .btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
</style>
