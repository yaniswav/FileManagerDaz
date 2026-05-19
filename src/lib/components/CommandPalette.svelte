<script lang="ts">
  import { tick } from 'svelte';
  import type { PaletteAction } from '$lib/commands/palette';
  import { filterActions } from '$lib/commands/palette';

  interface Props {
    actions: PaletteAction[];
    onclose: () => void;
  }

  let { actions, onclose }: Props = $props();

  let query = $state('');
  let selectedIndex = $state(0);
  let inputEl: HTMLInputElement | undefined = $state();
  let listEl: HTMLDivElement | undefined = $state();

  const filtered = $derived(filterActions(query, actions));

  const grouped = $derived.by(() => {
    const result: { group: string; actions: PaletteAction[]; indexBase: number }[] = [];
    let i = 0;
    let currentGroup: string | null = null;
    let bucket: PaletteAction[] = [];
    let base = 0;
    for (const a of filtered) {
      if (a.group !== currentGroup) {
        if (currentGroup !== null && bucket.length > 0) {
          result.push({ group: currentGroup, actions: bucket, indexBase: base });
        }
        currentGroup = a.group;
        bucket = [];
        base = i;
      }
      bucket.push(a);
      i++;
    }
    if (currentGroup !== null && bucket.length > 0) {
      result.push({ group: currentGroup, actions: bucket, indexBase: base });
    }
    return result;
  });

  $effect(() => {
    void filtered;
    if (selectedIndex >= filtered.length) {
      selectedIndex = filtered.length > 0 ? 0 : 0;
    }
  });

  $effect(() => {
    inputEl?.focus();
  });

  async function execute(action: PaletteAction) {
    onclose();
    await tick();
    try {
      await action.action();
    } catch (e) {
      if (import.meta.env.DEV) console.error('[Palette] action failed', e);
    }
  }

  function scrollToSelected() {
    void tick().then(() => {
      const el = listEl;
      if (!el) return;
      const target = el.querySelector(`[data-idx="${selectedIndex}"]`) as HTMLElement | null;
      target?.scrollIntoView({ block: 'nearest' });
    });
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.preventDefault();
      onclose();
    } else if (e.key === 'ArrowDown') {
      e.preventDefault();
      selectedIndex = Math.min(selectedIndex + 1, filtered.length - 1);
      scrollToSelected();
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      selectedIndex = Math.max(selectedIndex - 1, 0);
      scrollToSelected();
    } else if (e.key === 'Enter') {
      e.preventDefault();
      const action = filtered[selectedIndex];
      if (action) void execute(action);
    }
  }

  function handleBackdropClick(e: MouseEvent) {
    if (e.target === e.currentTarget) onclose();
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="palette-backdrop" onclick={handleBackdropClick}>
  <div class="palette-card" role="dialog" aria-modal="true" aria-label="Command palette">
    <input
      bind:this={inputEl}
      bind:value={query}
      onkeydown={handleKeydown}
      class="palette-input"
      type="text"
      placeholder="Type a command…"
      autocomplete="off"
      spellcheck="false"
    />
    <div bind:this={listEl} class="palette-list">
      {#if filtered.length === 0}
        <div class="palette-empty">No matching command</div>
      {:else}
        {#each grouped as g (g.group)}
          <div class="palette-group">{g.group}</div>
          {#each g.actions as action, i (action.id)}
            {@const globalIdx = g.indexBase + i}
            <button
              type="button"
              class="palette-item"
              class:selected={globalIdx === selectedIndex}
              data-idx={globalIdx}
              onmouseenter={() => (selectedIndex = globalIdx)}
              onclick={() => void execute(action)}
            >
              {#if action.icon}<span class="palette-icon">{action.icon}</span>{/if}
              <span class="palette-label">{action.label}</span>
            </button>
          {/each}
        {/each}
      {/if}
    </div>
    <div class="palette-footer">
      <span>↑↓ Navigate</span>
      <span>↵ Run</span>
      <span>Esc Close</span>
    </div>
  </div>
</div>

<style>
  .palette-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.5);
    backdrop-filter: blur(2px);
    z-index: 1000;
    display: flex;
    align-items: flex-start;
    justify-content: center;
    padding-top: 12vh;
  }

  .palette-card {
    width: 600px;
    max-width: 90vw;
    max-height: 70vh;
    display: flex;
    flex-direction: column;
    background: var(--bg-secondary, #1a1a2e);
    border-radius: 12px;
    border: 1px solid var(--border-color, #333);
    box-shadow: 0 24px 64px rgba(0, 0, 0, 0.5);
    overflow: hidden;
  }

  .palette-input {
    background: transparent;
    border: none;
    outline: none;
    padding: 1rem 1.25rem;
    color: var(--text-primary);
    font-size: 1rem;
    border-bottom: 1px solid var(--border-color, #333);
    width: 100%;
    box-sizing: border-box;
  }

  .palette-list {
    flex: 1;
    overflow-y: auto;
    padding: 0.25rem 0;
  }

  .palette-empty {
    padding: 1rem 1.25rem;
    color: var(--text-secondary);
    font-size: 0.9rem;
  }

  .palette-group {
    padding: 0.5rem 1.25rem 0.25rem;
    font-size: 0.7rem;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--text-secondary);
  }

  .palette-item {
    width: 100%;
    background: transparent;
    border: none;
    text-align: left;
    padding: 0.5rem 1.25rem;
    display: flex;
    align-items: center;
    gap: 0.75rem;
    color: var(--text-primary);
    cursor: pointer;
    font-size: 0.9rem;
    border-radius: 0;
    transition: background 0.1s;
  }

  .palette-item.selected {
    background: var(--accent, #8b5cf6);
    color: white;
  }

  .palette-icon {
    font-size: 1rem;
    width: 1.25rem;
    text-align: center;
  }

  .palette-label {
    flex: 1;
  }

  .palette-footer {
    display: flex;
    gap: 1rem;
    padding: 0.5rem 1.25rem;
    border-top: 1px solid var(--border-color, #333);
    font-size: 0.75rem;
    color: var(--text-secondary);
    background: var(--bg-primary, #0f0f1e);
  }
</style>
