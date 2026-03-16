<script lang="ts">
  import { t } from '$lib/i18n';
  import NormalizeFolder from '$lib/components/NormalizeFolder.svelte';
  import Maintenance from '$lib/components/Maintenance.svelte';
  import DuplicateDetector from '$lib/components/tools/DuplicateDetector.svelte';
  import SceneAnalyzer from '$lib/components/tools/SceneAnalyzer.svelte';
  import Downloads from '$lib/components/Downloads.svelte';

  type ToolId = 'normalize' | 'maintenance' | 'duplicates' | 'analyzer' | 'downloads' | 'migration';
  type ToolConfig = { id: ToolId; titleKey: string };

  const tools: ToolConfig[] = [
    { id: 'normalize',  titleKey: 'tools.normalize.title' },
    { id: 'maintenance', titleKey: 'tools.maintenance.title' },
    { id: 'duplicates', titleKey: 'tools.duplicates.title' },
    { id: 'analyzer',   titleKey: 'tools.analyzer.title' },
    { id: 'downloads',  titleKey: 'tools.downloads.title' },
    { id: 'migration',  titleKey: 'tools.migration.title' },
  ];

  let activeTool: ToolId = $state('normalize');
  let sourceLibrary = $state('');
  let destLibrary = $state('');
</script>

<div class="tools-panel">
  <div class="tools-body">
    <nav class="tools-nav" role="tablist" aria-label={$t('tools.title')}>
      <span class="nav-label">{$t('tools.title')}</span>
      {#each tools as tool}
        <button
          type="button"
          role="tab"
          class:active={activeTool === tool.id}
          aria-selected={activeTool === tool.id}
          onclick={() => (activeTool = tool.id)}
        >
          {$t(tool.titleKey)}
        </button>
      {/each}
    </nav>

    <section class="tools-content" role="tabpanel">
      {#if activeTool === 'normalize'}
        <NormalizeFolder embedded />
      {:else if activeTool === 'maintenance'}
        <Maintenance embedded />
      {:else if activeTool === 'duplicates'}
        <DuplicateDetector embedded />
      {:else if activeTool === 'analyzer'}
        <SceneAnalyzer embedded />
      {:else if activeTool === 'downloads'}
        <Downloads />
      {:else if activeTool === 'migration'}
        <div class="migration">
          <div class="migration-header">
            <h4>{$t('tools.migration.title')}</h4>
            <p>{$t('tools.migration.description')}</p>
          </div>

          <div class="migration-form">
            <label class="field">
              <span class="field-label">{$t('tools.migration.source')}</span>
              <select bind:value={sourceLibrary}>
                <option value="" disabled>{$t('tools.migration.selectLibrary')}</option>
              </select>
            </label>

            <span class="arrow">→</span>

            <label class="field">
              <span class="field-label">{$t('tools.migration.destination')}</span>
              <select bind:value={destLibrary}>
                <option value="" disabled>{$t('tools.migration.selectLibrary')}</option>
              </select>
            </label>
          </div>

          <button class="btn-migrate" disabled>
            {$t('tools.migration.migrate')}
          </button>

          <p class="migration-hint">{$t('tools.migration.hint')}</p>
        </div>
      {/if}
    </section>
  </div>
</div>

<style>
  .tools-panel {
    display: flex;
    flex-direction: column;
    height: 100%;
  }

  .tools-body {
    display: grid;
    grid-template-columns: 200px 1fr;
    gap: 0;
    flex: 1;
    min-height: 0;
  }

  /* ── Sidebar ── */
  .tools-nav {
    display: flex;
    flex-direction: column;
    gap: 0;
    padding: 0.75rem 0;
    border-right: 1px solid rgba(255, 255, 255, 0.06);
  }

  .nav-label {
    font-size: 0.65rem;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--text-secondary);
    opacity: 0.5;
    padding: 0 1rem 0.65rem;
  }

  .tools-nav button {
    background: transparent;
    color: var(--text-secondary);
    border: none;
    border-left: 2px solid transparent;
    border-radius: 0;
    padding: 0.5rem 1rem;
    cursor: pointer;
    text-align: left;
    font-size: 0.82rem;
    font-weight: 400;
    letter-spacing: -0.01em;
    transition: color 0.12s ease, border-color 0.12s ease;
    line-height: 1.4;
  }

  .tools-nav button:hover {
    color: var(--text-primary);
  }

  .tools-nav button.active {
    color: #fff;
    font-weight: 500;
    border-left-color: var(--accent);
  }

  /* ── Content ── */
  .tools-content {
    padding: 1.5rem 2rem;
    display: flex;
    flex-direction: column;
    gap: 1rem;
    min-width: 0;
    overflow-y: auto;
  }

  /* ── Action buttons inside child tools ── */
  .tools-content :global(button:not([role="tab"])) {
    border-radius: 6px;
    transition: filter 0.15s ease;
  }

  .tools-content :global(button:not([role="tab"]):hover) {
    filter: brightness(1.12);
  }

  .tools-content :global(button:not([role="tab"]):active) {
    filter: brightness(0.92);
  }

  /* ── Library Migration ── */
  .migration {
    display: flex;
    flex-direction: column;
    gap: 1.5rem;
    max-width: 520px;
  }

  .migration-header h4 {
    margin: 0;
    font-size: 1rem;
    font-weight: 600;
    color: var(--text-primary);
    letter-spacing: -0.02em;
  }

  .migration-header p {
    margin: 0.35rem 0 0;
    color: var(--text-secondary);
    font-size: 0.82rem;
    line-height: 1.55;
  }

  .migration-form {
    display: flex;
    align-items: flex-end;
    gap: 1rem;
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 0.35rem;
    flex: 1;
  }

  .field-label {
    font-size: 0.72rem;
    font-weight: 500;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--text-secondary);
  }

  .migration select {
    background: rgba(255, 255, 255, 0.04);
    color: var(--text-primary);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 6px;
    padding: 0.55rem 0.75rem;
    font-size: 0.82rem;
    cursor: pointer;
    appearance: none;
    width: 100%;
    transition: border-color 0.15s ease;
  }

  .migration select:focus {
    outline: none;
    border-color: var(--accent);
  }

  .arrow {
    color: var(--text-secondary);
    opacity: 0.4;
    font-size: 1.1rem;
    padding-bottom: 0.55rem;
    flex-shrink: 0;
  }

  .btn-migrate {
    background: var(--accent);
    color: #fff;
    border: none;
    border-radius: 6px;
    padding: 0.6rem 1.25rem;
    font-size: 0.82rem;
    font-weight: 500;
    cursor: not-allowed;
    opacity: 0.45;
    align-self: flex-start;
    transition: opacity 0.15s ease;
  }

  .migration-hint {
    font-size: 0.72rem;
    color: var(--text-secondary);
    opacity: 0.5;
    font-style: italic;
    margin: 0;
  }

  /* ── Responsive ── */
  @media (max-width: 900px) {
    .tools-body {
      grid-template-columns: 1fr;
    }

    .tools-nav {
      flex-direction: row;
      overflow-x: auto;
      border-right: none;
      border-bottom: 1px solid rgba(255, 255, 255, 0.06);
      padding: 0 0.5rem;
      gap: 0;
    }

    .nav-label {
      display: none;
    }

    .tools-nav button {
      border-left: none;
      border-bottom: 2px solid transparent;
      white-space: nowrap;
      padding: 0.6rem 0.85rem;
      font-size: 0.78rem;
    }

    .tools-nav button.active {
      border-bottom-color: var(--accent);
    }

    .tools-content {
      padding: 1.25rem;
    }

    .migration-form {
      flex-direction: column;
      align-items: stretch;
    }

    .arrow {
      text-align: center;
      padding: 0;
      transform: rotate(90deg);
    }
  }
</style>
