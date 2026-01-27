<script lang="ts">
  import { onMount } from 'svelte';
  import { open } from '@tauri-apps/plugin-dialog';
  import {
    getAppConfig,
    detectDazLibraries,
    addDazLibrary,
    removeDazLibrary,
    setDefaultLibrary,
    detectExternalTools,
    setTrashArchivesAfterImport,
    setDevLogExtractionTimings,
    setDevLogExtractionDetails,
    setLanguage,
    type AppConfig,
    type DazLibrary,
  } from '$lib/api/commands';
  import { t, locale, setLocale, LOCALE_NAMES, type Locale } from '$lib/i18n';

  let config: AppConfig | null = $state(null);
  let loading = $state(true);
  let error: string | null = $state(null);
  let detecting = $state(false);
  let detectingTools = $state(false);
  let savingTrashSetting = $state(false);
  let savingDevLogTimings = $state(false);
  let savingDevLogDetails = $state(false);
  let savingLanguage = $state(false);

  onMount(async () => {
    await loadConfig();
  });

  async function loadConfig() {
    loading = true;
    error = null;
    try {
      config = await getAppConfig();
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      loading = false;
    }
  }

  async function handleDetectLibraries() {
    detecting = true;
    error = null;
    try {
      const result = await detectDazLibraries();
      if (config) {
        config.dazLibraries = result.libraries;
      }
      if (result.newCount > 0) {
        // Feedback positif
      }
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      detecting = false;
    }
  }

  async function handleAddLibrary() {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: $t('settings.libraries.selectDialogTitle'),
      });

      if (selected && typeof selected === 'string') {
        const lib = await addDazLibrary(selected);
        if (config) {
          config.dazLibraries = [...config.dazLibraries, lib];
        }
      }
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
  }

  async function handleRemoveLibrary(path: string) {
    if (!confirm($t('settings.libraries.confirmRemove'))) return;

    try {
      await removeDazLibrary(path);
      if (config) {
        config.dazLibraries = config.dazLibraries.filter((l) => l.path !== path);
      }
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
  }

  async function handleSetDefault(path: string) {
    try {
      await setDefaultLibrary(path);
      if (config) {
        config.dazLibraries = config.dazLibraries.map((l) => ({
          ...l,
          isDefault: l.path === path,
        }));
        config.defaultDestination = path;
      }
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
  }

  async function handleDetectTools() {
    detectingTools = true;
    error = null;
    try {
      config = await detectExternalTools();
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      detectingTools = false;
    }
  }

  async function handleToggleTrashArchives() {
    if (!config) return;
    
    savingTrashSetting = true;
    error = null;
    const newValue = !config.trashArchivesAfterImport;
    
    try {
      await setTrashArchivesAfterImport(newValue);
      config.trashArchivesAfterImport = newValue;
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      savingTrashSetting = false;
    }
  }

  async function handleToggleDevLogTimings() {
    if (!config) return;
    
    savingDevLogTimings = true;
    error = null;
    const newValue = !config.devLogExtractionTimings;
    
    try {
      await setDevLogExtractionTimings(newValue);
      config.devLogExtractionTimings = newValue;
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      savingDevLogTimings = false;
    }
  }

  async function handleToggleDevLogDetails() {
    if (!config) return;
    
    savingDevLogDetails = true;
    error = null;
    const newValue = !config.devLogExtractionDetails;
    
    try {
      await setDevLogExtractionDetails(newValue);
      config.devLogExtractionDetails = newValue;
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      savingDevLogDetails = false;
    }
  }

  async function handleLanguageChange(newLang: Locale) {
    if (!config || savingLanguage) return;
    
    savingLanguage = true;
    error = null;
    
    try {
      await setLanguage(newLang);
      setLocale(newLang);
      config.language = newLang;
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      savingLanguage = false;
    }
  }
</script>

<div class="settings-page">
  {#if loading}
    <div class="loading">{$t('settings.loadingConfig')}</div>
  {:else if error}
    <div class="error-banner">
      <span>{$t('common.error')}: {error}</span>
      <button onclick={() => (error = null)}>X</button>
    </div>
  {/if}

  {#if config}
    <!-- Language Section -->
    <section class="settings-section">
      <div class="section-header">
        <h2>{$t('settings.language.title')}</h2>
      </div>

      <div class="language-selector">
        <p class="setting-description">{$t('settings.language.description')}</p>
        <div class="language-options">
          <button
            class="lang-btn"
            class:active={$locale === 'fr'}
            onclick={() => handleLanguageChange('fr')}
            disabled={savingLanguage}
          >
             {LOCALE_NAMES.fr}
          </button>
          <button
            class="lang-btn"
            class:active={$locale === 'en'}
            onclick={() => handleLanguageChange('en')}
            disabled={savingLanguage}
          >
             {LOCALE_NAMES.en}
          </button>
        </div>
      </div>
    </section>

    <!-- DAZ Libraries Section -->
    <section class="settings-section">
      <div class="section-header">
        <h2> {$t('settings.libraries.title')}</h2>
        <div class="section-actions">
          <button class="btn-secondary" onclick={handleDetectLibraries} disabled={detecting}>
            {#if detecting}
               {$t('settings.libraries.detecting')}
            {:else}
               {$t('settings.libraries.detectAuto')}
            {/if}
          </button>
          <button class="btn-primary" onclick={handleAddLibrary}>
            {$t('settings.libraries.add')}
          </button>
        </div>
      </div>

      {#if config.dazLibraries.length === 0}
        <div class="empty-state">
          <p>{$t('settings.libraries.noLibraries')}</p>
          <p class="hint">{$t('settings.libraries.noLibrariesHint')}</p>
        </div>
      {:else}
        <ul class="library-list">
          {#each config.dazLibraries as lib (lib.path)}
            <li class="library-item" class:not-found={!lib.exists}>
              <div class="library-info">
                <span class="library-name">
                  {lib.name}
                  {#if lib.isDefault}
                    <span class="default-badge">{$t('settings.libraries.defaultBadge')}</span>
                  {/if}
                </span>
                <span class="library-path">{lib.path}</span>
                {#if !lib.exists}
                  <span class="warning"> {$t('settings.libraries.pathNotFound')}</span>
                {/if}
              </div>
              <div class="library-actions">
                {#if !lib.isDefault && lib.exists}
                  <button class="btn-small" onclick={() => handleSetDefault(lib.path)} title={$t('settings.libraries.setDefault')}>
                    {$t('settings.libraries.setDefault')}
                  </button>
                {/if}
                <button class="btn-small btn-danger" onclick={() => handleRemoveLibrary(lib.path)} title={$t('settings.libraries.remove')}>
                  {$t('settings.libraries.remove')}
                </button>
              </div>
            </li>
          {/each}
        </ul>
      {/if}
    </section>

    <!-- Section Outils d'extraction -->
    <section class="settings-section">
      <div class="section-header">
        <h2>{$t('settings.externalTools.title')}</h2>
        <button class="btn-secondary" onclick={handleDetectTools} disabled={detectingTools}>
          {#if detectingTools}
            {$t('settings.externalTools.detecting')}
          {:else}
            {$t('settings.externalTools.redetect')}
          {/if}
        </button>
      </div>

      <div class="tools-grid">
        <div class="tool-card">
          <div class="tool-header">
            <span class="tool-name">ZIP</span>
            <span class="tool-status available"> {$t('settings.externalTools.builtIn')}</span>
          </div>
          <p class="tool-info">{$t('settings.externalTools.builtInHint')}</p>
        </div>

        <div class="tool-card">
          <div class="tool-header">
            <span class="tool-name">7z</span>
            <span class="tool-status" class:available={config.canExtract7z}>
              {#if config.canExtract7z}
                 {$t('settings.externalTools.available')}
              {:else}
                 {$t('settings.externalTools.notAvailable')}
              {/if}
            </span>
          </div>
          <p class="tool-info">
            {#if config.sevenzipPath}
              {$t('settings.externalTools.path', { path: config.sevenzipPath })}
            {:else}
              {$t('settings.externalTools.builtInExtractor')}
            {/if}
          </p>
        </div>

        <div class="tool-card">
          <div class="tool-header">
            <span class="tool-name">RAR</span>
            <span class="tool-status" class:available={config.canExtractRar}>
              {#if config.canExtractRar}
                 {$t('settings.externalTools.available')}
              {:else}
                 {$t('settings.externalTools.notAvailable')}
              {/if}
            </span>
          </div>
          <p class="tool-info">
            {#if config.unrarPath}
              {$t('settings.externalTools.path', { path: config.unrarPath })}
            {:else}
              <span class="warning">{$t('settings.externalTools.installWinrar')}</span>
            {/if}
          </p>
        </div>
      </div>
    </section>

    <!-- Post-import behavior section -->
    <section class="settings-section">
      <div class="section-header">
        <h2> {$t('settings.postImport.title')}</h2>
      </div>

      <div class="setting-row">
        <label class="checkbox-label">
          <input
            type="checkbox"
            checked={config.trashArchivesAfterImport}
            onchange={handleToggleTrashArchives}
            disabled={savingTrashSetting}
          />
          <span class="checkbox-text">
            {$t('settings.extraction.trashAfterImport')}
          </span>
        </label>
        <p class="setting-description">
          {$t('settings.extraction.trashAfterImportHint')}
        </p>
      </div>
    </section>

    <!-- System paths section -->
    <section class="settings-section">
      <div class="section-header">
        <h2>{$t('settings.paths.title')}</h2>
      </div>

      <div class="paths-list">
        <div class="path-item">
          <span class="path-label">{$t('settings.paths.database')}</span>
          <code>{config.databasePath}</code>
        </div>
        <div class="path-item">
          <span class="path-label">{$t('settings.paths.tempDir')}</span>
          <code>{config.tempDir}</code>
        </div>
        <div class="path-item">
          <span class="path-label">{$t('settings.paths.thumbnails')}</span>
          <code>{config.thumbnailsDir}</code>
        </div>
      </div>
    </section>

    <!-- Developer options section -->
    <section class="settings-section dev-section">
      <div class="section-header">
        <h2>{$t('settings.developer.title')}</h2>
      </div>

      <div class="setting-row">
        <label class="checkbox-label">
          <input
            type="checkbox"
            checked={config.devLogExtractionTimings}
            onchange={handleToggleDevLogTimings}
            disabled={savingDevLogTimings}
          />
          <span class="checkbox-text">
            {$t('settings.advanced.logTimings')}
          </span>
        </label>
        <p class="setting-description">
          {$t('settings.advanced.logTimingsHint')}
        </p>
      </div>

      <div class="setting-row">
        <label class="checkbox-label">
          <input
            type="checkbox"
            checked={config.devLogExtractionDetails}
            onchange={handleToggleDevLogDetails}
            disabled={savingDevLogDetails}
          />
          <span class="checkbox-text">
            {$t('settings.advanced.logMoves')}
          </span>
        </label>
        <p class="setting-description">
          {$t('settings.advanced.logMovesHint')}
        </p>
      </div>
    </section>
  {/if}
</div>

<style>
  .settings-page {
    display: flex;
    flex-direction: column;
    gap: 1.5rem;
  }

  .loading {
    text-align: center;
    padding: 2rem;
    color: var(--text-secondary);
  }

  .error-banner {
    background: rgba(239, 68, 68, 0.1);
    border: 1px solid var(--error);
    border-radius: 8px;
    padding: 0.75rem 1rem;
    display: flex;
    justify-content: space-between;
    align-items: center;
    color: var(--error);
  }

  .error-banner button {
    background: none;
    border: none;
    color: var(--error);
    cursor: pointer;
    padding: 0.25rem;
  }

  .settings-section {
    background: var(--bg-secondary);
    border-radius: 12px;
    padding: 1.25rem;
  }

  .section-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 1rem;
  }

  .section-header h2 {
    font-size: 1.1rem;
    margin: 0;
    color: var(--text-primary);
  }

  .section-actions {
    display: flex;
    gap: 0.5rem;
  }

  .btn-primary {
    background: var(--accent);
    color: white;
    border: none;
    padding: 0.5rem 1rem;
    border-radius: 6px;
    cursor: pointer;
    font-size: 0.875rem;
  }

  .btn-primary:hover {
    opacity: 0.9;
  }

  .btn-secondary {
    background: var(--bg-tertiary);
    color: var(--text-primary);
    border: 1px solid var(--border-color);
    padding: 0.5rem 1rem;
    border-radius: 6px;
    cursor: pointer;
    font-size: 0.875rem;
  }

  .btn-secondary:hover {
    background: var(--bg-hover);
  }

  .btn-secondary:disabled,
  .btn-primary:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .btn-small {
    background: none;
    border: none;
    padding: 0.25rem 0.5rem;
    cursor: pointer;
    font-size: 1rem;
    border-radius: 4px;
  }

  .btn-small:hover {
    background: var(--bg-tertiary);
  }

  .btn-danger:hover {
    background: rgba(239, 68, 68, 0.2);
  }

  .empty-state {
    text-align: center;
    padding: 2rem;
    color: var(--text-secondary);
  }

  .empty-state .hint {
    font-size: 0.875rem;
    margin-top: 0.5rem;
    opacity: 0.7;
  }

  .library-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  .library-item {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 0.75rem 1rem;
    background: var(--bg-tertiary);
    border-radius: 8px;
    border: 1px solid transparent;
  }

  .library-item.not-found {
    border-color: var(--warning);
    opacity: 0.7;
  }

  .library-info {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    min-width: 0;
    flex: 1;
  }

  .library-name {
    font-weight: 500;
    color: var(--text-primary);
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }

  .default-badge {
    background: var(--accent);
    color: white;
    font-size: 0.7rem;
    padding: 0.1rem 0.4rem;
    border-radius: 4px;
    font-weight: normal;
  }

  .library-path {
    font-size: 0.8rem;
    color: var(--text-secondary);
    font-family: monospace;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .library-actions {
    display: flex;
    gap: 0.25rem;
  }

  .warning {
    color: var(--warning);
    font-size: 0.8rem;
  }

  .tools-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
    gap: 1rem;
  }

  .tool-card {
    background: var(--bg-tertiary);
    border-radius: 8px;
    padding: 1rem;
  }

  .tool-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 0.5rem;
  }

  .tool-name {
    font-weight: 600;
    font-size: 1rem;
  }

  .tool-status {
    font-size: 0.8rem;
    color: var(--error);
  }

  .tool-status.available {
    color: var(--success);
  }

  .tool-info {
    font-size: 0.8rem;
    color: var(--text-secondary);
    margin: 0;
    word-break: break-all;
  }

  .paths-list {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }

  .path-item {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
  }

  .path-item .path-label {
    font-size: 0.85rem;
    color: var(--text-secondary);
  }

  .path-item code {
    font-size: 0.8rem;
    background: var(--bg-tertiary);
    padding: 0.5rem 0.75rem;
    border-radius: 4px;
    color: var(--text-primary);
    word-break: break-all;
  }

  /* Developer section styling */
  .dev-section {
    border: 1px dashed var(--border-color);
    opacity: 0.85;
  }

  .dev-section h2 {
    font-size: 0.95rem;
  }

  /* Language selector */
  .language-selector {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }

  .language-options {
    display: flex;
    gap: 0.75rem;
  }

  .lang-btn {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.75rem 1.25rem;
    background: var(--bg-tertiary);
    border: 2px solid transparent;
    border-radius: 8px;
    cursor: pointer;
    font-size: 1rem;
    color: var(--text-primary);
    transition: all 0.2s ease;
  }

  .lang-btn:hover:not(:disabled) {
    background: var(--bg-hover);
    border-color: var(--border-color);
  }

  .lang-btn.active {
    background: var(--accent);
    color: white;
    border-color: var(--accent);
  }

  .lang-btn:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }
</style>

