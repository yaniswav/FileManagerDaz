<script lang="ts">
  import { t } from '$lib/i18n';
  import NormalizeFolder from '$lib/components/NormalizeFolder.svelte';
  import Maintenance from '$lib/components/Maintenance.svelte';
  import DuplicateDetector from '$lib/components/tools/DuplicateDetector.svelte';
  import SceneAnalyzer from '$lib/components/tools/SceneAnalyzer.svelte';
  import Downloads from '$lib/components/Downloads.svelte';
  import CustomSelect from '$lib/components/ui/CustomSelect.svelte';

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
              <CustomSelect
                bind:value={sourceLibrary}
                placeholder={$t('tools.migration.selectLibrary')}
                options={[]}
              />
            </label>

            <span class="arrow">→</span>

            <label class="field">
              <span class="field-label">{$t('tools.migration.destination')}</span>
              <CustomSelect
                bind:value={destLibrary}
                placeholder={$t('tools.migration.selectLibrary')}
                options={[]}
              />
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
    grid-template-columns: 210px 1fr;
    gap: 0;
    flex: 1;
    min-height: 0;
  }

  /* ── Sidebar ── */
  .tools-nav {
    display: flex;
    flex-direction: column;
    gap: 1px;
    padding: 1rem 0.5rem;
    border-right: 1px solid var(--surface-border);
  }

  .nav-label {
    font-size: 0.6rem;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.1em;
    color: var(--text-muted);
    padding: 0 0.75rem 0.75rem;
  }

  .tools-nav button {
    background: transparent;
    color: var(--text-secondary);
    border: none;
    border-radius: 6px;
    padding: 0.5rem 0.75rem;
    cursor: pointer;
    text-align: left;
    font-size: 0.82rem;
    font-weight: 400;
    letter-spacing: -0.01em;
    line-height: 1.4;
    position: relative;
    transition: all var(--duration-normal) var(--ease-out);
  }

  .tools-nav button::before {
    content: '';
    position: absolute;
    left: 0;
    top: 50%;
    transform: translateY(-50%) scaleY(0);
    width: 2px;
    height: 60%;
    border-radius: 1px;
    background: var(--accent);
    box-shadow: 0 0 8px var(--accent-glow);
    transition: transform var(--duration-normal) var(--ease-spring);
  }

  .tools-nav button:hover {
    color: var(--text-primary);
    background: var(--surface-glass);
    transform: translateX(2px);
    box-shadow: none;
  }

  .tools-nav button.active {
    color: #fff;
    font-weight: 500;
    background: var(--surface-elevated);
  }

  .tools-nav button.active::before {
    transform: translateY(-50%) scaleY(1);
  }

  /* ── Content ── */
  .tools-content {
    padding: 2rem;
    display: flex;
    flex-direction: column;
    gap: 1.25rem;
    min-width: 0;
    overflow-y: auto;
  }

  /* ── Child tool buttons ── */
  .tools-content :global(button:not([role="tab"])) {
    border-radius: 6px;
    transition: all var(--duration-normal) var(--ease-out);
  }

  .tools-content :global(button:not([role="tab"]):hover:not(:disabled)) {
    transform: translateY(-1px) scale(1.01);
    box-shadow: 0 4px 12px -2px var(--accent-glow);
  }

  .tools-content :global(button:not([role="tab"]):active:not(:disabled)) {
    transform: translateY(0) scale(0.99);
    box-shadow: none;
  }

  /* ── Library Migration ── */
  .migration {
    display: flex;
    flex-direction: column;
    gap: 1.75rem;
    max-width: 560px;
  }

  .migration-header h4 {
    margin: 0;
    font-size: 1.05rem;
    font-weight: 600;
    color: var(--text-primary);
    letter-spacing: -0.02em;
  }

  .migration-header p {
    margin: 0.4rem 0 0;
    color: var(--text-secondary);
    font-size: 0.82rem;
    line-height: 1.6;
  }

  .migration-form {
    display: flex;
    align-items: flex-end;
    gap: 1rem;
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
    flex: 1;
  }

  .field-label {
    font-size: 0.7rem;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--text-muted);
  }

  .migration select {
    background: var(--surface-glass);
    color: var(--text-primary);
    border: 1px solid var(--surface-border);
    border-radius: 6px;
    padding: 0.55rem 0.75rem;
    font-size: 0.82rem;
    cursor: pointer;
    appearance: none;
    width: 100%;
    transition: all var(--duration-fast) var(--ease-out);
  }

  .migration select:hover {
    border-color: rgba(255, 255, 255, 0.15);
    background: var(--surface-elevated);
  }

  .migration select:focus {
    outline: none;
    border-color: var(--accent);
    box-shadow: 0 0 0 3px var(--accent-glow);
  }

  .arrow {
    color: var(--text-muted);
    font-size: 1.1rem;
    padding-bottom: 0.55rem;
    flex-shrink: 0;
  }

  .btn-migrate {
    align-self: flex-start;
    opacity: 0.35;
    cursor: not-allowed;
  }

  .btn-migrate:hover {
    transform: none;
    box-shadow: none;
  }

  .migration-hint {
    font-size: 0.72rem;
    color: var(--text-muted);
    font-style: italic;
    margin: 0;
    line-height: 1.6;
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
      border-bottom: 1px solid var(--surface-border);
      padding: 0.25rem 0.5rem;
      gap: 2px;
    }

    .nav-label {
      display: none;
    }

    .tools-nav button {
      white-space: nowrap;
      padding: 0.55rem 0.85rem;
      font-size: 0.78rem;
    }

    .tools-nav button::before {
      left: 50%;
      top: auto;
      bottom: 0;
      transform: translateX(-50%) scaleX(0);
      width: 60%;
      height: 2px;
    }

    .tools-nav button.active::before {
      transform: translateX(-50%) scaleX(1);
    }

    .tools-nav button:hover {
      transform: none;
    }

    .tools-content {
      padding: 1.5rem;
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
