<script lang="ts">
  import { onMount } from 'svelte';

  interface Option {
    value: string;
    label: string;
  }

  let {
    options = [],
    value = $bindable(''),
    onchange = undefined,
    placeholder = 'Select…',
    disabled = false,
    title = '',
    id = '',
    class: className = ''
  }: {
    options: Option[];
    value: string;
    onchange?: ((value: string) => void) | undefined;
    placeholder?: string;
    disabled?: boolean;
    title?: string;
    id?: string;
    class?: string;
  } = $props();

  let open = $state(false);
  let focusedIndex = $state(-1);
  let triggerEl: HTMLButtonElement;
  let listEl: HTMLDivElement;
  let wrapperEl: HTMLDivElement;

  const selectedLabel = $derived(
    options.find(o => o.value === value)?.label ?? placeholder
  );

  function toggle() {
    if (disabled) return;
    open = !open;
    if (open) {
      focusedIndex = options.findIndex(o => o.value === value);
      if (focusedIndex < 0) focusedIndex = 0;
    }
  }

  function select(opt: Option) {
    value = opt.value;
    open = false;
    onchange?.(opt.value);
    triggerEl?.focus();
  }

  function handleKeydown(e: KeyboardEvent) {
    if (disabled) return;

    switch (e.key) {
      case 'Enter':
      case ' ':
        e.preventDefault();
        if (open && focusedIndex >= 0) {
          select(options[focusedIndex]);
        } else {
          toggle();
        }
        break;
      case 'ArrowDown':
        e.preventDefault();
        if (!open) { toggle(); break; }
        focusedIndex = Math.min(focusedIndex + 1, options.length - 1);
        scrollToFocused();
        break;
      case 'ArrowUp':
        e.preventDefault();
        if (!open) { toggle(); break; }
        focusedIndex = Math.max(focusedIndex - 1, 0);
        scrollToFocused();
        break;
      case 'Escape':
        e.preventDefault();
        open = false;
        triggerEl?.focus();
        break;
      case 'Home':
        if (open) { e.preventDefault(); focusedIndex = 0; scrollToFocused(); }
        break;
      case 'End':
        if (open) { e.preventDefault(); focusedIndex = options.length - 1; scrollToFocused(); }
        break;
    }
  }

  function scrollToFocused() {
    requestAnimationFrame(() => {
      listEl?.querySelector('.focused')?.scrollIntoView({ block: 'nearest' });
    });
  }

  function handleClickOutside(e: MouseEvent) {
    if (open && wrapperEl && !wrapperEl.contains(e.target as Node)) {
      open = false;
    }
  }

  onMount(() => {
    document.addEventListener('mousedown', handleClickOutside);
    return () => document.removeEventListener('mousedown', handleClickOutside);
  });
</script>

<div
  class="custom-select {className}"
  class:open
  class:disabled
  bind:this={wrapperEl}
>
  <button
    type="button"
    class="select-trigger"
    {id}
    {title}
    {disabled}
    bind:this={triggerEl}
    onclick={toggle}
    onkeydown={handleKeydown}
    aria-haspopup="listbox"
    aria-expanded={open}
  >
    <span class="select-label" class:placeholder={!options.find(o => o.value === value)}>
      {selectedLabel}
    </span>
    <svg class="select-chevron" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
      <path d="M6 9l6 6 6-6"/>
    </svg>
  </button>

  {#if open}
    <div class="select-dropdown" bind:this={listEl} role="listbox">
      {#each options as opt, i}
        <button
          type="button"
          class="select-option"
          class:selected={opt.value === value}
          class:focused={i === focusedIndex}
          role="option"
          aria-selected={opt.value === value}
          onclick={() => select(opt)}
          onmouseenter={() => focusedIndex = i}
        >
          {opt.label}
          {#if opt.value === value}
            <svg class="check-icon" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5">
              <path d="M5 13l4 4L19 7"/>
            </svg>
          {/if}
        </button>
      {/each}
    </div>
  {/if}
</div>

<style>
  .custom-select {
    position: relative;
    display: inline-flex;
    min-width: 0;
  }

  .select-trigger {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    width: 100%;
    padding: 0.5rem 0.75rem;
    padding-right: 2rem;
    background: var(--bg-secondary);
    color: var(--text-primary);
    border: 1px solid var(--surface-border);
    border-radius: 6px;
    font-size: 0.85rem;
    font-family: inherit;
    font-weight: 400;
    cursor: pointer;
    text-align: left;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    transition: all var(--duration-fast) var(--ease-out);
    position: relative;
    box-shadow: none;
    transform: none;
  }

  .select-trigger:hover:not(:disabled) {
    border-color: rgba(255, 255, 255, 0.15);
    background: var(--bg-tertiary);
    transform: none;
    box-shadow: none;
  }

  .custom-select.open .select-trigger {
    border-color: var(--accent);
    box-shadow: 0 0 0 3px var(--accent-glow);
    transform: none;
  }

  .select-trigger:disabled {
    opacity: 0.4;
    cursor: not-allowed;
    transform: none;
    box-shadow: none;
  }

  .select-label {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .select-label.placeholder {
    color: var(--text-muted);
  }

  .select-chevron {
    position: absolute;
    right: 0.6rem;
    top: 50%;
    transform: translateY(-50%) rotate(0deg);
    color: var(--text-muted);
    transition: transform var(--duration-fast) var(--ease-out);
    flex-shrink: 0;
  }

  .custom-select.open .select-chevron {
    transform: translateY(-50%) rotate(180deg);
  }

  .select-dropdown {
    position: absolute;
    top: calc(100% + 4px);
    left: 0;
    min-width: 100%;
    max-height: 260px;
    overflow-y: auto;
    background: var(--bg-secondary);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 8px;
    padding: 4px;
    z-index: 999;
    box-shadow:
      0 8px 32px -4px rgba(0, 0, 0, 0.6),
      0 0 0 1px rgba(255, 255, 255, 0.06);
    backdrop-filter: blur(16px) saturate(150%);
    animation: dropdown-in 0.15s var(--ease-out);
  }

  @keyframes dropdown-in {
    from {
      opacity: 0;
      transform: translateY(-4px) scale(0.98);
    }
    to {
      opacity: 1;
      transform: translateY(0) scale(1);
    }
  }

  .select-option {
    display: flex;
    align-items: center;
    justify-content: space-between;
    width: 100%;
    padding: 0.45rem 0.65rem;
    background: transparent;
    color: var(--text-secondary);
    border: none;
    border-radius: 5px;
    font-size: 0.82rem;
    font-family: inherit;
    font-weight: 400;
    cursor: pointer;
    text-align: left;
    transition: all 0.1s ease;
    white-space: nowrap;
    transform: none;
    box-shadow: none;
  }

  .select-option:hover,
  .select-option.focused {
    background: var(--surface-elevated);
    color: var(--text-primary);
    transform: none;
    box-shadow: none;
  }

  .select-option.selected {
    color: var(--accent);
    font-weight: 500;
  }

  .check-icon {
    color: var(--accent);
    flex-shrink: 0;
    margin-left: 0.5rem;
  }

  .custom-select.disabled {
    pointer-events: none;
  }
</style>
