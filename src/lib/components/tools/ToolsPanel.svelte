<script lang="ts">
  import { t } from '$lib/i18n';
  import NormalizeFolder from '$lib/components/NormalizeFolder.svelte';
  import Maintenance from '$lib/components/Maintenance.svelte';
  import DuplicateDetector from '$lib/components/tools/DuplicateDetector.svelte';
  import SceneAnalyzer from '$lib/components/tools/SceneAnalyzer.svelte';

  type ToolId = 'normalize' | 'maintenance' | 'duplicates' | 'analyzer';
  type ToolConfig = { id: ToolId; titleKey: string; descriptionKey: string };

  const tools: ToolConfig[] = [
    {
      id: 'normalize',
      titleKey: 'tools.normalize.title',
      descriptionKey: 'tools.normalize.description',
    },
    {
      id: 'maintenance',
      titleKey: 'tools.maintenance.title',
      descriptionKey: 'tools.maintenance.description',
    },
    {
      id: 'duplicates',
      titleKey: 'tools.duplicates.title',
      descriptionKey: 'tools.duplicates.description',
    },
    {
      id: 'analyzer',
      titleKey: 'tools.analyzer.title',
      descriptionKey: 'tools.analyzer.description',
    },
  ];

  let activeTool: ToolId = $state('normalize');

  let activeInfo = $derived.by(() => tools.find((tool) => tool.id === activeTool) ?? tools[0]);
</script>

<div class="tools-panel">
  <header class="tools-header">
    <div>
      <h3>{$t('tools.title')}</h3>
      <p class="subtitle">{$t('tools.subtitle')}</p>
    </div>
  </header>

  <div class="tools-body">
    <aside class="tools-nav" role="tablist" aria-label={$t('tools.title')}>
      {#each tools as tool}
        <button
          type="button"
          role="tab"
          class:active={activeTool === tool.id}
          aria-selected={activeTool === tool.id}
          onclick={() => (activeTool = tool.id)}
        >
          <span class="tool-name">{$t(tool.titleKey)}</span>
          <span class="tool-desc">{$t(tool.descriptionKey)}</span>
        </button>
      {/each}
    </aside>

    <section class="tools-content" role="tabpanel">
      <div class="tool-intro">
        <h4>{$t(activeInfo.titleKey)}</h4>
        <p>{$t(activeInfo.descriptionKey)}</p>
      </div>

      {#if activeTool === 'normalize'}
        <NormalizeFolder embedded />
      {:else if activeTool === 'maintenance'}
        <Maintenance embedded />
      {:else if activeTool === 'duplicates'}
        <DuplicateDetector embedded />
      {:else if activeTool === 'analyzer'}
        <SceneAnalyzer embedded />
      {/if}
    </section>
  </div>
</div>

<style>
  .tools-panel {
    display: flex;
    flex-direction: column;
    gap: 1rem;
  }

  .tools-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 1rem;
  }

  h3 {
    margin: 0;
    color: var(--accent);
  }

  .subtitle {
    margin: 0.25rem 0 0;
    color: var(--text-secondary);
    font-size: 0.85rem;
  }

  .tools-body {
    display: grid;
    grid-template-columns: minmax(220px, 280px) 1fr;
    gap: 1rem;
  }

  .tools-nav {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  .tools-nav button {
    background: var(--bg-secondary);
    color: var(--text-primary);
    border: 1px solid var(--border-color);
    border-radius: 12px;
    padding: 0.75rem;
    cursor: pointer;
    text-align: left;
    display: flex;
    flex-direction: column;
    gap: 0.35rem;
  }

  .tools-nav button:hover {
    background: var(--bg-hover);
  }

  .tools-nav button.active {
    background: rgba(79, 70, 229, 0.2);
    border-color: var(--accent);
  }

  .tool-name {
    font-weight: 600;
  }

  .tool-desc {
    font-size: 0.8rem;
    color: var(--text-secondary);
    display: -webkit-box;
    -webkit-line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }

  .tools-content {
    background: var(--bg-secondary);
    border-radius: 12px;
    padding: 1rem;
    display: flex;
    flex-direction: column;
    gap: 1rem;
    min-width: 0;
  }

  .tool-intro h4 {
    margin: 0;
    color: var(--text-primary);
  }

  .tool-intro p {
    margin: 0.35rem 0 0;
    color: var(--text-secondary);
    font-size: 0.9rem;
  }

  @media (max-width: 900px) {
    .tools-body {
      grid-template-columns: 1fr;
    }
  }
</style>
