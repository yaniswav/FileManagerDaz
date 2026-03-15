<script lang="ts">
  import {
    listCollections,
    createCollection,
    addToCollection,
    type Collection,
  } from '$lib/api/commands';
  import { addToast } from '$lib/stores/toast.svelte';

  interface Props {
    productIds: number[];
    onclose: () => void;
    onadded: () => void;
  }

  let { productIds, onclose, onadded }: Props = $props();

  let collections: Collection[] = $state([]);
  let loading = $state(true);
  let adding = $state(false);
  let newName = $state('');
  let creating = $state(false);

  $effect(() => {
    loadCollections();
  });

  async function loadCollections() {
    loading = true;
    try {
      collections = await listCollections();
    } catch (e) {
      addToast(String(e), 'error', 'Failed to load collections');
    } finally {
      loading = false;
    }
  }

  async function handleCreate() {
    const name = newName.trim();
    if (!name) return;
    creating = true;
    try {
      const col = await createCollection(name);
      collections = [...collections, col];
      newName = '';
      addToast(`Collection "${col.name}" created`, 'success');
    } catch (e) {
      addToast(String(e), 'error', 'Failed to create collection');
    } finally {
      creating = false;
    }
  }

  async function handleAdd(collection: Collection) {
    adding = true;
    try {
      const count = await addToCollection(collection.id, productIds);
      addToast(`${count} product(s) added to "${collection.name}"`, 'success');
      onadded();
    } catch (e) {
      addToast(String(e), 'error', 'Failed to add to collection');
    } finally {
      adding = false;
    }
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="overlay" onclick={onclose}>
  <div class="modal" onclick={(e) => e.stopPropagation()}>
    <h3>📂 Add to Collection</h3>
    <p class="subtitle">{productIds.length} product(s) selected</p>

    <!-- Create new -->
    <div class="create-row">
      <input
        type="text"
        bind:value={newName}
        placeholder="New collection name..."
        onkeydown={(e) => e.key === 'Enter' && handleCreate()}
      />
      <button
        type="button"
        class="btn-create"
        onclick={handleCreate}
        disabled={!newName.trim() || creating}
      >
        {creating ? '...' : '+ Create'}
      </button>
    </div>

    <!-- Existing collections list -->
    <div class="collection-list">
      {#if loading}
        <p class="hint">Loading...</p>
      {:else if collections.length === 0}
        <p class="hint">No collections yet. Create one above!</p>
      {:else}
        {#each collections as col (col.id)}
          <button
            type="button"
            class="collection-item"
            onclick={() => handleAdd(col)}
            disabled={adding}
          >
            <span class="col-name">📁 {col.name}</span>
            <span class="col-count">{col.itemCount} items</span>
          </button>
        {/each}
      {/if}
    </div>

    <div class="actions">
      <button type="button" class="btn-cancel" onclick={onclose}>Cancel</button>
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
    width: 420px;
    max-width: 90vw;
    max-height: 70vh;
    display: flex;
    flex-direction: column;
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

  .create-row {
    display: flex;
    gap: 0.5rem;
    margin-bottom: 0.75rem;
  }

  .create-row input {
    flex: 1;
    padding: 0.5rem 0.75rem;
    border: 1px solid var(--border-color);
    border-radius: 10px;
    background: var(--bg-primary);
    color: var(--text-primary);
    font-size: 0.85rem;
  }

  .create-row input:focus {
    outline: none;
    border-color: var(--accent);
  }

  .btn-create {
    padding: 0.5rem 0.75rem;
    border: none;
    border-radius: 10px;
    background: var(--accent, #8b5cf6);
    color: white;
    font-size: 0.82rem;
    font-weight: 600;
    cursor: pointer;
    white-space: nowrap;
    transition: opacity 0.15s;
  }

  .btn-create:hover { opacity: 0.9; }
  .btn-create:disabled { opacity: 0.5; cursor: not-allowed; }

  .collection-list {
    flex: 1;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 0.35rem;
    max-height: 300px;
    margin-bottom: 0.75rem;
  }

  .collection-item {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 0.6rem 0.75rem;
    border: 1px solid var(--border-color);
    border-radius: 10px;
    background: var(--bg-primary);
    color: var(--text-primary);
    cursor: pointer;
    transition: all 0.15s;
    font-size: 0.85rem;
  }

  .collection-item:hover {
    border-color: var(--accent, #8b5cf6);
    background: var(--bg-hover);
  }

  .collection-item:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .col-name {
    font-weight: 500;
  }

  .col-count {
    font-size: 0.75rem;
    color: var(--text-secondary);
  }

  .hint {
    font-size: 0.82rem;
    color: var(--text-secondary);
    text-align: center;
    padding: 1rem 0;
  }

  .actions {
    display: flex;
    justify-content: flex-end;
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
</style>
