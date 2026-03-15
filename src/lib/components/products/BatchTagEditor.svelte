<script lang="ts">
  import { batchUpdateTags, parseTags } from '$lib/api/commands';
  import { addToast } from '$lib/stores/toast.svelte';

  interface Props {
    selectedIds: number[];
    onclose: () => void;
    onapplied: () => void;
  }

  let { selectedIds, onclose, onapplied }: Props = $props();

  type TagMode = 'add' | 'remove' | 'replace';

  let tagInput = $state('');
  let mode = $state<TagMode>('add');
  let loading = $state(false);
  let error: string | null = $state(null);

  let tags = $derived(parseTags(tagInput));
  let canApply = $derived(tags.length > 0 || mode === 'replace');

  const MODE_INFO: Record<TagMode, { label: string; desc: string; chipClass: string; prefix: string }> = {
    add:     { label: '➕ Add',     desc: 'Merge tags into existing ones',   chipClass: 'add',     prefix: '+' },
    remove:  { label: '➖ Remove',  desc: 'Remove these tags if present',    chipClass: 'remove',  prefix: '−' },
    replace: { label: '🔄 Replace', desc: 'Replace all tags with these',    chipClass: 'replace', prefix: '=' },
  };

  async function handleApply() {
    if (!canApply) return;
    loading = true;
    error = null;
    try {
      const count = await batchUpdateTags(selectedIds, tags, mode);
      addToast(`Tags updated on ${count} product(s)`, 'success');
      onapplied();
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      error = msg;
      addToast(msg, 'error', 'Tag update failed');
    } finally {
      loading = false;
    }
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="overlay" onclick={onclose}>
  <div class="modal" onclick={(e) => e.stopPropagation()}>
    <h3>🏷️ Batch Tag Editor</h3>
    <p class="subtitle">{selectedIds.length} product(s) selected</p>

    {#if error}
      <div class="error-box">{error}</div>
    {/if}

    <!-- Mode selector -->
    <div class="mode-selector">
      {#each (['add', 'remove', 'replace'] as TagMode[]) as m}
        <button
          type="button"
          class="mode-btn"
          class:active={mode === m}
          onclick={() => (mode = m)}
        >
          {MODE_INFO[m].label}
        </button>
      {/each}
    </div>
    <p class="mode-desc">{MODE_INFO[mode].desc}</p>

    <div class="field">
      <label for="tag-input">Tags <span class="hint">(comma-separated)</span></label>
      <input
        id="tag-input"
        type="text"
        bind:value={tagInput}
        placeholder="fantasy, favorite, hd..."
      />
      {#if tags.length > 0}
        <div class="preview">
          {#each tags as tag}
            <span class="chip {MODE_INFO[mode].chipClass}">{MODE_INFO[mode].prefix} {tag}</span>
          {/each}
        </div>
      {/if}
    </div>

    <div class="actions">
      <button type="button" class="btn-cancel" onclick={onclose}>Cancel</button>
      <button
        type="button"
        class="btn-apply"
        onclick={handleApply}
        disabled={!canApply || loading}
      >
        {#if loading}
          Applying…
        {:else}
          Apply to {selectedIds.length} product(s)
        {/if}
      </button>
    </div>
  </div>
</div>

<style>
  .overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.6);
    backdrop-filter: blur(4px);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 9999;
    animation: fadeIn 0.15s ease;
  }

  @keyframes fadeIn {
    from { opacity: 0; }
    to { opacity: 1; }
  }

  .modal {
    background: var(--bg-secondary);
    border: 1px solid var(--border-color);
    border-radius: 16px;
    padding: 1.5rem;
    width: 440px;
    max-width: 90vw;
    box-shadow: 0 20px 60px rgba(0, 0, 0, 0.5);
    animation: slideUp 0.2s ease;
  }

  @keyframes slideUp {
    from { transform: translateY(20px); opacity: 0; }
    to { transform: translateY(0); opacity: 1; }
  }

  h3 {
    margin: 0 0 0.25rem;
    font-size: 1.15rem;
    color: var(--text-primary);
  }

  .subtitle {
    margin: 0 0 1rem;
    font-size: 0.85rem;
    color: var(--text-secondary);
  }

  .error-box {
    padding: 0.5rem 0.75rem;
    border-radius: 8px;
    background: rgba(239, 68, 68, 0.1);
    color: #ef4444;
    font-size: 0.82rem;
    margin-bottom: 0.75rem;
  }

  .field {
    margin-bottom: 1rem;
  }

  .field label {
    display: block;
    font-size: 0.85rem;
    font-weight: 600;
    color: var(--text-primary);
    margin-bottom: 0.35rem;
  }

  .hint {
    font-weight: 400;
    color: var(--text-secondary);
    font-size: 0.78rem;
  }

  .field input {
    width: 100%;
    padding: 0.5rem 0.75rem;
    border: 1px solid var(--border-color);
    border-radius: 10px;
    background: var(--bg-primary);
    color: var(--text-primary);
    font-size: 0.9rem;
    box-sizing: border-box;
  }

  .field input:focus {
    outline: none;
    border-color: var(--accent);
  }

  .preview {
    display: flex;
    flex-wrap: wrap;
    gap: 0.3rem;
    margin-top: 0.4rem;
  }

  .chip {
    padding: 2px 8px;
    border-radius: 999px;
    font-size: 0.72rem;
    font-weight: 600;
  }

  .chip.add {
    background: rgba(16, 185, 129, 0.15);
    color: #10b981;
  }

  .chip.remove {
    background: rgba(239, 68, 68, 0.15);
    color: #ef4444;
  }

  .chip.replace {
    background: rgba(59, 130, 246, 0.15);
    color: #3b82f6;
  }

  .mode-selector {
    display: flex;
    gap: 0.4rem;
    margin-bottom: 0.35rem;
  }

  .mode-btn {
    flex: 1;
    padding: 0.4rem 0.5rem;
    border: 1px solid var(--border-color);
    border-radius: 8px;
    background: var(--bg-primary);
    color: var(--text-secondary);
    font-size: 0.78rem;
    cursor: pointer;
    transition: all 0.15s;
  }

  .mode-btn:hover {
    background: var(--bg-hover);
  }

  .mode-btn.active {
    background: var(--accent, #8b5cf6);
    color: white;
    border-color: var(--accent, #8b5cf6);
    font-weight: 600;
  }

  .mode-desc {
    margin: 0 0 0.75rem;
    font-size: 0.75rem;
    color: var(--text-secondary);
    font-style: italic;
  }

  .actions {
    display: flex;
    gap: 0.5rem;
    justify-content: flex-end;
    margin-top: 0.5rem;
  }

  .btn-cancel {
    padding: 0.5rem 1rem;
    border: 1px solid var(--border-color);
    border-radius: 10px;
    background: var(--bg-tertiary);
    color: var(--text-primary);
    font-size: 0.85rem;
    cursor: pointer;
  }

  .btn-cancel:hover {
    background: var(--bg-hover);
  }

  .btn-apply {
    padding: 0.5rem 1rem;
    border: none;
    border-radius: 10px;
    background: var(--accent, #8b5cf6);
    color: white;
    font-size: 0.85rem;
    font-weight: 600;
    cursor: pointer;
    transition: opacity 0.15s;
  }

  .btn-apply:hover { opacity: 0.9; }
  .btn-apply:disabled { opacity: 0.5; cursor: not-allowed; }
</style>
