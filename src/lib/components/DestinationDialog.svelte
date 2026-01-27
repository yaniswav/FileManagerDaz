<script lang="ts">
  import { onMount } from 'svelte';
  import { t } from '$lib/i18n';
  import { 
    type DestinationProposal, 
    type DestinationAlternative,
    type ContentType,
    proposeDestination,
    moveToCustomDestination,
  } from '$lib/api/commands';

  // Props
  interface Props {
    /** Chemin temporaire du contenu extrait */
    tempPath: string;
    /** Nom de la source (pour affichage) */
    sourceName: string;
    /** Callback quand le déplacement est terminé */
    oncomplete?: (destinationPath: string) => void;
    /** Callback quand le dialogue est annulé */
    oncancel?: () => void;
    /** Callback en cas d'erreur */
    onerror?: (message: string) => void;
  }

  let { tempPath, sourceName, oncomplete, oncancel, onerror }: Props = $props();

  // States
  let loading = $state(true);
  let moving = $state(false);
  let proposal: DestinationProposal | null = $state(null);
  let error = $state<string | null>(null);
  
  // User selection
  let selectedPath = $state('');
  let customPath = $state('');
  let useCustom = $state(false);

  // Charge la proposition de destination
  onMount(async () => {
    try {
      const prop = await proposeDestination(tempPath);
      proposal = prop;
      selectedPath = prop.recommendedPath;
      customPath = prop.recommendedPath;
    } catch (e) {
      error = e instanceof Error ? e.message : $t('destination.errors.unknown');
    } finally {
      loading = false;
    }
  });

  // Handle alternative selection
  function selectAlternative(alt: DestinationAlternative) {
    selectedPath = alt.path;
    customPath = alt.path;
    useCustom = false;
  }

  // Handle full path selection
  function selectFullPath(path: string) {
    selectedPath = path;
    customPath = path;
    useCustom = false;
  }

  // Enable custom mode
  function enableCustom() {
    useCustom = true;
    selectedPath = customPath;
  }

  // Validate and move
  async function handleConfirm() {
    if (moving) return;

    const finalPath = useCustom ? customPath : selectedPath;
    if (!finalPath) {
      error = $t('destination.errors.selectDestination');
      return;
    }

    moving = true;
    error = null;

    try {
      const result = await moveToCustomDestination(tempPath, finalPath);
      
      if (result.success) {
        oncomplete?.(result.destinationPath);
      } else {
        error = result.errors.join('\n') || $t('destination.errors.moveFailed');
      }
    } catch (e) {
      error = e instanceof Error ? e.message : $t('destination.errors.moveError');
      onerror?.(error);
    } finally {
      moving = false;
    }
  }

  // Annule le dialogue
  function handleCancel() {
    oncancel?.();
  }

  // Formate un type de contenu
  function formatContentType(type: ContentType | null): string {
    if (!type) return $t('common.unknown');
    const key = `products.contentTypes.${type}`;
    const translated = $t(key);
    return translated !== key ? translated : type;
  }

  // Formats path for display
  function truncatePath(path: string, maxLen: number = 60): string {
    if (path.length <= maxLen) return path;
    return '...' + path.slice(-(maxLen - 3));
  }

  // Converts confidence to percentage
  function confidencePercent(conf: number): string {
    return Math.round(conf * 100) + '%';
  }

  // CSS class based on confidence level
  function confidenceClass(conf: number): string {
    if (conf >= 0.8) return 'high';
    if (conf >= 0.5) return 'medium';
    return 'low';
  }
</script>

<div class="dialog-overlay" role="dialog" aria-modal="true" aria-labelledby="dialog-title">
  <div class="dialog-container">
    <header class="dialog-header">
      <h2 id="dialog-title">📂 {$t('destination.title')}</h2>
      <p class="dialog-subtitle">
        <strong>{sourceName}</strong>
      </p>
    </header>

    <div class="dialog-content">
      {#if loading}
        <div class="loading-state">
          <div class="spinner"></div>
          <p>{$t('destination.analyzing')}</p>
        </div>
      {:else if error && !proposal}
        <div class="error-state">
          <span class="error-icon">❌</span>
          <p>{error}</p>
          <button type="button" class="btn-secondary" onclick={handleCancel}>{$t('common.close')}</button>
        </div>
      {:else if proposal}
        <!-- Analysis summary -->
        <section class="analysis-summary">
          <div class="summary-header">
            <span class="content-type-badge">{formatContentType(proposal.contentType)}</span>
            <span class="confidence-badge {confidenceClass(proposal.confidence)}">
              {$t('destination.confidence', { percent: confidencePercent(proposal.confidence) })}
            </span>
          </div>
          <p class="summary-reason">{proposal.reason}</p>
        </section>

        <!-- Recommended destination -->
        <section class="recommendation">
          <h3>{$t('destination.recommendedTitle')}</h3>
          <button 
            type="button" 
            class="destination-option recommended" 
            class:selected={selectedPath === proposal?.recommendedPath && !useCustom}
            onclick={() => proposal && selectFullPath(proposal.recommendedPath)}
          >
            <span class="option-icon">⭐</span>
            <span class="option-path" title={proposal?.recommendedPath}>
              {truncatePath(proposal?.recommendedPath ?? '')}
            </span>
          </button>
        </section>

        <!-- Alternatives -->
        {#if proposal.alternatives.length > 0}
          <section class="alternatives">
            <h3>{$t('destination.alternativesTitle')}</h3>
            <div class="alternatives-list">
              {#each proposal.alternatives as alt}
                <button 
                  type="button" 
                  class="destination-option" 
                  class:selected={selectedPath === alt.path && !useCustom}
                  onclick={() => selectAlternative(alt)}
                >
                  <span class="option-icon">📁</span>
                  <div class="option-details">
                    <span class="option-label">{alt.label}</span>
                    <span class="option-path" title={alt.path}>
                      {truncatePath(alt.path, 50)}
                    </span>
                  </div>
                  <span class="option-confidence {confidenceClass(alt.confidence)}">
                    {confidencePercent(alt.confidence)}
                  </span>
                </button>
              {/each}
            </div>
          </section>
        {/if}

        <!-- All suggested paths -->
        {#if proposal.fullPaths.length > 1}
          <section class="full-paths">
            <h3>{$t('destination.fullPathsTitle')}</h3>
            <div class="paths-list">
              {#each proposal.fullPaths as path}
                <button 
                  type="button" 
                  class="path-option" 
                  class:selected={selectedPath === path && !useCustom}
                  onclick={() => selectFullPath(path)}
                >
                  {truncatePath(path)}
                </button>
              {/each}
            </div>
          </section>
        {/if}

        <!-- Custom path -->
        <section class="custom-path">
          <h3>
            <label for="custom-input">
              <input 
                type="checkbox" 
                id="custom-check"
                checked={useCustom}
                onchange={() => useCustom = !useCustom}
              />
              {$t('destination.customPath')}
            </label>
          </h3>
          {#if useCustom}
            <input 
              type="text" 
              id="custom-input"
              class="custom-input"
              bind:value={customPath}
              placeholder={$t('destination.customPlaceholder')}
            />
          {/if}
        </section>

        <!-- Move error -->
        {#if error}
          <div class="move-error">
            <span class="error-icon">⚠️</span>
            <p>{error}</p>
          </div>
        {/if}
      {/if}
    </div>

    <footer class="dialog-footer">
      <button type="button" class="btn-secondary" onclick={handleCancel} disabled={moving}>
        {$t('common.cancel')}
      </button>
      <button 
        type="button" 
        class="btn-primary" 
        onclick={handleConfirm} 
        disabled={loading || moving || (!selectedPath && !customPath)}
      >
        {#if moving}
          <span class="btn-spinner"></span>
          {$t('destination.moving')}
        {:else}
          ✓ {$t('common.confirm')}
        {/if}
      </button>
    </footer>
  </div>
</div>

<style>
  .dialog-overlay {
    position: fixed;
    top: 0;
    left: 0;
    right: 0;
    bottom: 0;
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
    width: 90%;
    max-width: 700px;
    max-height: 85vh;
    display: flex;
    flex-direction: column;
    box-shadow: 0 20px 60px rgba(0, 0, 0, 0.5);
  }

  .dialog-header {
    padding: 1.5rem;
    border-bottom: 1px solid var(--border-color, #333);
  }

  .dialog-header h2 {
    margin: 0;
    font-size: 1.25rem;
    color: var(--text-primary, #fff);
  }

  .dialog-subtitle {
    margin: 0.5rem 0 0;
    font-size: 0.9rem;
    color: var(--text-secondary, #aaa);
  }

  .dialog-content {
    flex: 1;
    padding: 1.5rem;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 1.5rem;
  }

  .dialog-footer {
    padding: 1rem 1.5rem;
    border-top: 1px solid var(--border-color, #333);
    display: flex;
    gap: 1rem;
    justify-content: flex-end;
  }

  /* Loading/Error states */
  .loading-state, .error-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 1rem;
    padding: 2rem;
    text-align: center;
  }

  .spinner, .btn-spinner {
    width: 24px;
    height: 24px;
    border: 3px solid var(--border-color, #333);
    border-top-color: var(--accent-color, #6366f1);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }

  .btn-spinner {
    width: 16px;
    height: 16px;
    border-width: 2px;
    display: inline-block;
    margin-right: 0.5rem;
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }

  .error-icon {
    font-size: 2rem;
  }

  /* Analysis Summary */
  .analysis-summary {
    background: var(--bg-secondary, #1a1a2e);
    border-radius: 8px;
    padding: 1rem;
  }

  .summary-header {
    display: flex;
    gap: 0.75rem;
    margin-bottom: 0.5rem;
  }

  .content-type-badge {
    background: var(--accent-color, #6366f1);
    color: white;
    padding: 0.25rem 0.75rem;
    border-radius: 999px;
    font-size: 0.8rem;
    font-weight: 500;
  }

  .confidence-badge {
    padding: 0.25rem 0.75rem;
    border-radius: 999px;
    font-size: 0.8rem;
    font-weight: 500;
  }

  .confidence-badge.high { background: #22c55e20; color: #22c55e; }
  .confidence-badge.medium { background: #f59e0b20; color: #f59e0b; }
  .confidence-badge.low { background: #ef444420; color: #ef4444; }

  .summary-reason {
    margin: 0;
    color: var(--text-secondary, #aaa);
    font-size: 0.9rem;
  }

  /* Sections */
  section h3 {
    margin: 0 0 0.75rem;
    font-size: 0.9rem;
    color: var(--text-secondary, #888);
    font-weight: 500;
  }

  /* Destination options */
  .destination-option {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    width: 100%;
    padding: 0.75rem 1rem;
    background: var(--bg-secondary, #1a1a2e);
    border: 2px solid transparent;
    border-radius: 8px;
    cursor: pointer;
    transition: all 0.15s ease;
    text-align: left;
    font-family: inherit;
    font-size: inherit;
    color: var(--text-primary, #fff);
  }

  .destination-option:hover {
    background: var(--bg-hover, #252545);
  }

  .destination-option.selected {
    border-color: var(--accent-color, #6366f1);
    background: var(--accent-color, #6366f1)10;
  }

  .destination-option.recommended {
    border-color: #22c55e50;
  }

  .destination-option.recommended.selected {
    border-color: #22c55e;
    background: #22c55e10;
  }

  .option-icon {
    font-size: 1.25rem;
    flex-shrink: 0;
  }

  .option-details {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
  }

  .option-label {
    font-weight: 500;
    color: var(--text-primary, #fff);
  }

  .option-path {
    font-size: 0.85rem;
    color: var(--text-secondary, #888);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .option-confidence {
    font-size: 0.75rem;
    padding: 0.2rem 0.5rem;
    border-radius: 4px;
    flex-shrink: 0;
  }

  .option-confidence.high { background: #22c55e20; color: #22c55e; }
  .option-confidence.medium { background: #f59e0b20; color: #f59e0b; }
  .option-confidence.low { background: #ef444420; color: #ef4444; }

  /* Alternatives list */
  .alternatives-list {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  /* Full paths */
  .paths-list {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem;
  }

  .path-option {
    padding: 0.5rem 0.75rem;
    background: var(--bg-secondary, #1a1a2e);
    border: 1px solid var(--border-color, #333);
    border-radius: 6px;
    font-size: 0.85rem;
    color: var(--text-secondary, #888);
    cursor: pointer;
    transition: all 0.15s ease;
    font-family: inherit;
  }

  .path-option:hover {
    background: var(--bg-hover, #252545);
    color: var(--text-primary, #fff);
  }

  .path-option.selected {
    border-color: var(--accent-color, #6366f1);
    background: var(--accent-color, #6366f1)10;
    color: var(--text-primary, #fff);
  }

  /* Custom path */
  .custom-path h3 label {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    cursor: pointer;
  }

  .custom-path h3 input[type="checkbox"] {
    width: 1rem;
    height: 1rem;
    cursor: pointer;
  }

  .custom-input {
    width: 100%;
    padding: 0.75rem;
    background: var(--bg-secondary, #1a1a2e);
    border: 1px solid var(--border-color, #333);
    border-radius: 8px;
    color: var(--text-primary, #fff);
    font-family: monospace;
    font-size: 0.9rem;
  }

  .custom-input:focus {
    outline: none;
    border-color: var(--accent-color, #6366f1);
  }

  /* Move error */
  .move-error {
    display: flex;
    align-items: flex-start;
    gap: 0.75rem;
    padding: 1rem;
    background: #ef444420;
    border: 1px solid #ef444450;
    border-radius: 8px;
    color: #ef4444;
  }

  /* Buttons */
  .btn-primary, .btn-secondary {
    padding: 0.75rem 1.5rem;
    border-radius: 8px;
    font-size: 0.95rem;
    font-weight: 500;
    cursor: pointer;
    transition: all 0.15s ease;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    font-family: inherit;
  }

  .btn-primary {
    background: var(--accent-color, #6366f1);
    border: none;
    color: white;
  }

  .btn-primary:hover:not(:disabled) {
    background: var(--accent-hover, #4f46e5);
  }

  .btn-primary:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .btn-secondary {
    background: transparent;
    border: 1px solid var(--border-color, #333);
    color: var(--text-primary, #fff);
  }

  .btn-secondary:hover:not(:disabled) {
    background: var(--bg-secondary, #1a1a2e);
  }

  .btn-secondary:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
</style>


