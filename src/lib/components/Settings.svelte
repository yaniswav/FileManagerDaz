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
    setMinimizeToTray,
    setAutoImportEnabled,
    setAutoImportFolder,
    setAutoImportMode,
    setCloseAction,
    startWatching,
    stopWatching,
    getDownloadsFolder,
    type AppConfig,
    type DazLibrary,
  } from '$lib/api/commands';
  import { t, locale, setLocale, LOCALE_NAMES, type Locale } from '$lib/i18n';
  import { getVersion } from '@tauri-apps/api/app';
  import { checkForUpdates, updaterState } from '$lib/api/updater';

  let config: AppConfig | null = $state(null);
  let loading = $state(true);
  let error: string | null = $state(null);
  let detecting = $state(false);
  let detectingTools = $state(false);
  let savingTrashSetting = $state(false);
  let savingDevLogTimings = $state(false);
  let savingDevLogDetails = $state(false);
  let appVersion = $state('');
  let savingLanguage = $state(false);
  let savingMinimizeToTray = $state(false);
  let savingAutoImport = $state(false);
  let savingAutoImportMode = $state(false);
  let savingCloseAction = $state(false);

  // Collapsible sections
  let showDevSection = $state(false);

  onMount(async () => {
    await loadConfig();
    appVersion = await getVersion();
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

  async function handleToggleAutoImport() {
    if (!config) return;
    savingAutoImport = true;
    error = null;
    const newValue = !config.autoImportEnabled;
    try {
      await setAutoImportEnabled(newValue);
      config = { ...config, autoImportEnabled: newValue };

      if (newValue) {
        // Start the watcher on the configured folder (or Downloads)
        const folder = config.autoImportFolder || (await getDownloadsFolder()) || '';
        if (folder) {
          await startWatching(folder);
        }
      } else {
        await stopWatching();
      }
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      savingAutoImport = false;
    }
  }

  async function handlePickAutoImportFolder() {
    if (!config) return;
    error = null;
    try {
      const selected = await open({ directory: true, multiple: false, title: 'Select auto-import folder' });
      if (selected) {
        const folder = typeof selected === 'string' ? selected : selected[0];
        if (folder) {
          await setAutoImportFolder(folder);
          config = { ...config, autoImportFolder: folder };
          // Restart watcher on new folder
          if (config.autoImportEnabled) {
            await stopWatching();
            await startWatching(folder);
          }
        }
      }
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
  }

  async function handleClearAutoImportFolder() {
    if (!config) return;
    error = null;
    try {
      // Stop the watcher since there's no folder to watch
      if (config.autoImportEnabled) {
        await stopWatching();
      }
      await setAutoImportFolder(null);
      config = { ...config, autoImportFolder: null };
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
  }

  async function handleAutoImportModeChange(mode: string) {
    if (!config || savingAutoImportMode) return;
    savingAutoImportMode = true;
    error = null;
    try {
      await setAutoImportMode(mode);
      config = { ...config, autoImportMode: mode };
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      savingAutoImportMode = false;
    }
  }

  async function handleCloseActionChange(action: string) {
    if (!config || savingCloseAction) return;
    savingCloseAction = true;
    error = null;
    try {
      await setCloseAction(action);
      config.closeAction = action;
      config.minimizeToTray = action === 'minimize';
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      savingCloseAction = false;
    }
  }
</script>

<div class="settings-page">
  {#if loading}
    <div class="loading-container">
      <div class="loading-spinner"></div>
      <p>{$t('settings.loadingConfig')}</p>
    </div>
  {:else if error}
    <div class="error-banner">
      <span class="error-icon">⚠️</span>
      <span class="error-text">{$t('common.error')}: {error}</span>
      <button class="error-dismiss" onclick={() => (error = null)}>✕</button>
    </div>
  {/if}

  {#if config}
    <!-- ═══ GENERAL SECTION ═══ -->
    <section class="settings-card">
      <div class="card-header">
        <div class="card-title">
          <span class="card-icon">🌐</span>
          <h2>{$t('settings.language.title')}</h2>
        </div>
      </div>
      <div class="card-body">
        <p class="card-description">{$t('settings.language.description')}</p>
        <div class="language-pills">
          <button
            class="pill"
            class:pill-active={$locale === 'fr'}
            onclick={() => handleLanguageChange('fr')}
            disabled={savingLanguage}
          >
            🇫🇷 {LOCALE_NAMES.fr}
          </button>
          <button
            class="pill"
            class:pill-active={$locale === 'en'}
            onclick={() => handleLanguageChange('en')}
            disabled={savingLanguage}
          >
            🇬🇧 {LOCALE_NAMES.en}
          </button>
        </div>
      </div>
    </section>

    <!-- ═══ DAZ LIBRARIES ═══ -->
    <section class="settings-card">
      <div class="card-header">
        <div class="card-title">
          <span class="card-icon">📚</span>
          <h2>{$t('settings.libraries.title')}</h2>
        </div>
        <div class="card-actions">
          <button class="btn-outline" onclick={handleDetectLibraries} disabled={detecting}>
            {#if detecting}
              <span class="btn-spinner"></span>
            {:else}
              🔍
            {/if}
            {$t('settings.libraries.detectAuto')}
          </button>
          <button class="btn-filled" onclick={handleAddLibrary}>
            ＋ {$t('settings.libraries.add')}
          </button>
        </div>
      </div>
      <div class="card-body">
        {#if config.dazLibraries.length === 0}
          <div class="empty-state">
            <span class="empty-icon">📂</span>
            <p>{$t('settings.libraries.noLibraries')}</p>
            <p class="empty-hint">{$t('settings.libraries.noLibrariesHint')}</p>
          </div>
        {:else}
          <div class="library-grid">
            {#each config.dazLibraries as lib (lib.path)}
              <div class="library-card" class:library-missing={!lib.exists}>
                <div class="library-header">
                  <span class="library-name">
                    {lib.name}
                    {#if lib.isDefault}
                      <span class="badge badge-accent">★ {$t('settings.libraries.defaultBadge')}</span>
                    {/if}
                  </span>
                  {#if !lib.exists}
                    <span class="badge badge-warning">⚠ {$t('settings.libraries.pathNotFound')}</span>
                  {/if}
                </div>
                <code class="library-path">{lib.path}</code>
                <div class="library-footer">
                  {#if !lib.isDefault && lib.exists}
                    <button class="btn-subtle" onclick={() => handleSetDefault(lib.path)}>
                      ★ {$t('settings.libraries.setDefault')}
                    </button>
                  {/if}
                  <button class="btn-subtle btn-subtle-danger" onclick={() => handleRemoveLibrary(lib.path)}>
                    🗑 {$t('settings.libraries.remove')}
                  </button>
                </div>
              </div>
            {/each}
          </div>
        {/if}
      </div>
    </section>

    <!-- ═══ EXTRACTION TOOLS ═══ -->
    <section class="settings-card">
      <div class="card-header">
        <div class="card-title">
          <span class="card-icon">🔧</span>
          <h2>{$t('settings.externalTools.title')}</h2>
        </div>
        <button class="btn-outline" onclick={handleDetectTools} disabled={detectingTools}>
          {#if detectingTools}
            <span class="btn-spinner"></span>
          {:else}
            🔄
          {/if}
          {$t('settings.externalTools.redetect')}
        </button>
      </div>
      <div class="card-body">
        <div class="tools-row">
          <div class="tool-chip">
            <span class="tool-icon">📦</span>
            <div class="tool-details">
              <span class="tool-label">ZIP</span>
              <span class="tool-meta">{$t('settings.externalTools.builtIn')}</span>
            </div>
            <span class="status-dot status-green"></span>
          </div>

          <div class="tool-chip">
            <span class="tool-icon">📦</span>
            <div class="tool-details">
              <span class="tool-label">7z</span>
              <span class="tool-meta">
                {#if config.sevenzipPath}
                  {config.sevenzipPath}
                {:else}
                  {$t('settings.externalTools.builtInExtractor')}
                {/if}
              </span>
            </div>
            <span class="status-dot" class:status-green={config.canExtract7z} class:status-yellow={!config.canExtract7z}></span>
          </div>

          <div class="tool-chip">
            <span class="tool-icon">📦</span>
            <div class="tool-details">
              <span class="tool-label">RAR</span>
              <span class="tool-meta">
                {#if config.unrarPath}
                  {config.unrarPath}
                {:else}
                  {$t('settings.externalTools.installWinrar')}
                {/if}
              </span>
            </div>
            <span class="status-dot" class:status-green={config.canExtractRar} class:status-red={!config.canExtractRar}></span>
          </div>
        </div>
      </div>
    </section>

    <!-- ═══ BEHAVIOR ═══ -->
    <section class="settings-card">
      <div class="card-header">
        <div class="card-title">
          <span class="card-icon">⚙️</span>
          <h2>{$t('settings.postImport.title')}</h2>
        </div>
      </div>
      <div class="card-body">
        <div class="toggle-row" onclick={handleToggleTrashArchives} role="button" tabindex="0" onkeydown={(e) => e.key === 'Enter' && handleToggleTrashArchives()}>
          <div class="toggle-info">
            <span class="toggle-label">{$t('settings.extraction.trashAfterImport')}</span>
            <span class="toggle-hint">{$t('settings.extraction.trashAfterImportHint')}</span>
          </div>
          <div class="toggle-switch" class:toggle-on={config.trashArchivesAfterImport}>
            <div class="toggle-knob"></div>
          </div>
        </div>
      </div>
    </section>

    <!-- ═══ SYSTEM TRAY & BACKGROUND ═══ -->
    <section class="settings-card">
      <div class="card-header">
        <div class="card-title">
          <span class="card-icon">🖥️</span>
          <h2>{$t('settings.systemTray.title')}</h2>
        </div>
      </div>
      <div class="card-body">
        <!-- Close Action -->
        <div class="setting-group">
          <span class="group-label">{$t('settings.systemTray.closeAction')}</span>
          <div class="radio-cards">
            <button
              class="radio-card"
              class:radio-active={config.closeAction === 'ask'}
              onclick={() => handleCloseActionChange('ask')}
              disabled={savingCloseAction}
            >
              <span class="radio-icon">❓</span>
              <span class="radio-label">{$t('settings.systemTray.closeActionAsk')}</span>
            </button>
            <button
              class="radio-card"
              class:radio-active={config.closeAction === 'minimize'}
              onclick={() => handleCloseActionChange('minimize')}
              disabled={savingCloseAction}
            >
              <span class="radio-icon">📥</span>
              <span class="radio-label">{$t('settings.systemTray.closeActionMinimize')}</span>
            </button>
            <button
              class="radio-card"
              class:radio-active={config.closeAction === 'quit'}
              onclick={() => handleCloseActionChange('quit')}
              disabled={savingCloseAction}
            >
              <span class="radio-icon">🚪</span>
              <span class="radio-label">{$t('settings.systemTray.closeActionQuit')}</span>
            </button>
          </div>
        </div>

        <div class="divider"></div>

        <!-- Auto-Import -->
        <div class="toggle-row" onclick={handleToggleAutoImport} role="button" tabindex="0" onkeydown={(e) => e.key === 'Enter' && handleToggleAutoImport()}>
          <div class="toggle-info">
            <span class="toggle-label">{$t('settings.autoImport.enabled')}</span>
            <span class="toggle-hint">{$t('settings.autoImport.enabledHint')}</span>
          </div>
          <div class="toggle-switch" class:toggle-on={config.autoImportEnabled}>
            <div class="toggle-knob"></div>
          </div>
        </div>

        {#if config.autoImportEnabled}
          <div class="sub-setting">
            <span class="sub-label">{$t('settings.autoImport.folder')}</span>
            <div class="folder-picker">
              {#if config.autoImportFolder}
                <code class="folder-path">{config.autoImportFolder}</code>
                <button class="btn-icon-danger" onclick={handleClearAutoImportFolder} title="Clear">✕</button>
              {:else}
                <span class="text-muted">{$t('settings.autoImport.noFolder')}</span>
              {/if}
              <button class="btn-outline btn-sm" onclick={handlePickAutoImportFolder}>
                📂 {$t('settings.autoImport.pickFolder')}
              </button>
            </div>
          </div>

          <div class="sub-setting">
            <span class="sub-label">{$t('settings.autoImport.modeLabel')}</span>
            <span class="toggle-hint">{$t('settings.autoImport.modeHint')}</span>
            <div class="radio-cards">
              <button
                class="radio-card"
                class:radio-active={config.autoImportMode === 'watch_only'}
                onclick={() => handleAutoImportModeChange('watch_only')}
                disabled={savingAutoImportMode}
              >
                <span class="radio-icon">👁️</span>
                <span class="radio-label">{$t('settings.autoImport.modeWatchOnly')}</span>
                <span class="radio-hint">{$t('settings.autoImport.modeWatchOnlyHint')}</span>
              </button>
              <button
                class="radio-card"
                class:radio-active={config.autoImportMode === 'confirm'}
                onclick={() => handleAutoImportModeChange('confirm')}
                disabled={savingAutoImportMode}
              >
                <span class="radio-icon">❓</span>
                <span class="radio-label">{$t('settings.autoImport.modeConfirm')}</span>
                <span class="radio-hint">{$t('settings.autoImport.modeConfirmHint')}</span>
              </button>
              <button
                class="radio-card"
                class:radio-active={config.autoImportMode === 'auto'}
                onclick={() => handleAutoImportModeChange('auto')}
                disabled={savingAutoImportMode}
              >
                <span class="radio-icon">⚡</span>
                <span class="radio-label">{$t('settings.autoImport.modeAuto')}</span>
                <span class="radio-hint">{$t('settings.autoImport.modeAutoHint')}</span>
              </button>
            </div>
          </div>
        {/if}
      </div>
    </section>

    <!-- ═══ SYSTEM PATHS (Read-only) ═══ -->
    <section class="settings-card">
      <div class="card-header">
        <div class="card-title">
          <span class="card-icon">📁</span>
          <h2>{$t('settings.paths.title')}</h2>
        </div>
      </div>
      <div class="card-body">
        <div class="paths-grid">
          <div class="path-row">
            <span class="path-label">{$t('settings.paths.database')}</span>
            <code class="path-value">{config.databasePath}</code>
          </div>
          <div class="path-row">
            <span class="path-label">{$t('settings.paths.tempDir')}</span>
            <code class="path-value">{config.tempDir}</code>
          </div>
          <div class="path-row">
            <span class="path-label">{$t('settings.paths.thumbnails')}</span>
            <code class="path-value">{config.thumbnailsDir}</code>
          </div>
        </div>
      </div>
    </section>

    <!-- ═══ DEVELOPER (Collapsible) ═══ -->
    <section class="settings-card dev-card">
      <button class="card-header card-header-collapsible" onclick={() => showDevSection = !showDevSection}>
        <div class="card-title">
          <span class="card-icon">🛠️</span>
          <h2>{$t('settings.developer.title')}</h2>
        </div>
        <span class="collapse-arrow" class:collapse-open={showDevSection}>▶</span>
      </button>
      {#if showDevSection}
        <div class="card-body">
          <div class="toggle-row" onclick={handleToggleDevLogTimings} role="button" tabindex="0" onkeydown={(e) => e.key === 'Enter' && handleToggleDevLogTimings()}>
            <div class="toggle-info">
              <span class="toggle-label">{$t('settings.advanced.logTimings')}</span>
              <span class="toggle-hint">{$t('settings.advanced.logTimingsHint')}</span>
            </div>
            <div class="toggle-switch" class:toggle-on={config.devLogExtractionTimings}>
              <div class="toggle-knob"></div>
            </div>
          </div>

          <div class="toggle-row" onclick={handleToggleDevLogDetails} role="button" tabindex="0" onkeydown={(e) => e.key === 'Enter' && handleToggleDevLogDetails()}>
            <div class="toggle-info">
              <span class="toggle-label">{$t('settings.advanced.logMoves')}</span>
              <span class="toggle-hint">{$t('settings.advanced.logMovesHint')}</span>
            </div>
            <div class="toggle-switch" class:toggle-on={config.devLogExtractionDetails}>
              <div class="toggle-knob"></div>
            </div>
          </div>
        </div>
      {/if}
    </section>
  {/if}

  <!-- ═══ ABOUT ═══ -->
  <section class="settings-card">
    <div class="card-header">
      <div class="card-title">
        <span class="card-icon">ℹ️</span>
        <h2>{$t('settings.about.title')}</h2>
      </div>
    </div>
    <div class="card-body">
      <div class="about-row">
        <span class="about-label">{$t('settings.about.version')}</span>
        <span class="about-value">v{appVersion}</span>
      </div>
      <div class="about-row">
        <button
          class="btn-update"
          onclick={() => checkForUpdates(false)}
          disabled={updaterState.checking || updaterState.downloading}
        >
          {#if updaterState.downloading}
            ⏳ {$t('settings.about.downloading')} ({updaterState.progress}%)
          {:else if updaterState.checking}
            🔍 {$t('settings.about.checking')}
          {:else}
            🔄 {$t('settings.about.checkUpdates')}
          {/if}
        </button>
      </div>
    </div>
  </section>
</div>

<style>
  /* ═══ Page Layout ═══ */
  .settings-page {
    display: flex;
    flex-direction: column;
    gap: 1rem;
    padding-bottom: 2rem;
  }

  /* ═══ Loading ═══ */
  .loading-container {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 1rem;
    padding: 3rem;
    color: var(--text-secondary);
  }

  .loading-spinner {
    width: 32px;
    height: 32px;
    border: 3px solid var(--bg-tertiary);
    border-top: 3px solid var(--accent);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }

  /* ═══ Error Banner ═══ */
  .error-banner {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    padding: 0.75rem 1rem;
    background: rgba(233, 69, 96, 0.1);
    border: 1px solid rgba(233, 69, 96, 0.3);
    border-radius: 10px;
    color: var(--error);
    font-size: 0.9rem;
  }

  .error-icon { font-size: 1.2rem; flex-shrink: 0; }
  .error-text { flex: 1; }

  .error-dismiss {
    background: none !important;
    border: none !important;
    color: var(--error) !important;
    cursor: pointer;
    padding: 0.25rem 0.5rem;
    font-size: 1rem;
    opacity: 0.7;
  }

  .error-dismiss:hover {
    opacity: 1;
    background: none !important;
  }

  /* ═══ Card ═══ */
  .settings-card {
    background: var(--bg-secondary);
    border: 1px solid rgba(255, 255, 255, 0.05);
    border-radius: 14px;
    overflow: hidden;
    transition: border-color 0.2s;
  }

  .settings-card:hover {
    border-color: rgba(255, 255, 255, 0.1);
  }

  .card-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 1rem 1.25rem;
    background: rgba(255, 255, 255, 0.02);
    border-bottom: 1px solid rgba(255, 255, 255, 0.05);
  }

  .card-header-collapsible {
    width: 100%;
    cursor: pointer;
    background: rgba(255, 255, 255, 0.02) !important;
    border-radius: 0 !important;
    text-align: left;
  }

  .card-header-collapsible:hover {
    background: rgba(255, 255, 255, 0.04) !important;
  }

  .card-title {
    display: flex;
    align-items: center;
    gap: 0.6rem;
  }

  .card-icon {
    font-size: 1.2rem;
  }

  .card-title h2 {
    margin: 0;
    font-size: 1rem;
    font-weight: 600;
    color: var(--text-primary);
  }

  .card-actions {
    display: flex;
    gap: 0.5rem;
  }

  .card-body {
    padding: 1.25rem;
    display: flex;
    flex-direction: column;
    gap: 1rem;
  }

  .card-description {
    margin: 0;
    font-size: 0.85rem;
    color: var(--text-secondary);
  }

  /* ═══ Collapse Arrow ═══ */
  .collapse-arrow {
    font-size: 0.75rem;
    color: var(--text-secondary);
    transition: transform 0.2s ease;
  }

  .collapse-open {
    transform: rotate(90deg);
  }

  /* ═══ Buttons ═══ */
  .btn-filled {
    background: var(--accent);
    color: white;
    border: none;
    padding: 0.45rem 1rem;
    border-radius: 8px;
    cursor: pointer;
    font-size: 0.85rem;
    font-weight: 500;
    transition: filter 0.15s;
  }

  .btn-filled:hover { filter: brightness(1.15); }

  .btn-outline {
    background: transparent;
    color: var(--text-primary);
    border: 1px solid var(--border-color);
    padding: 0.45rem 1rem;
    border-radius: 8px;
    cursor: pointer;
    font-size: 0.85rem;
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
    transition: all 0.15s;
  }

  .btn-outline:hover {
    border-color: var(--accent);
    background: rgba(233, 69, 96, 0.05);
  }

  .btn-outline:disabled, .btn-filled:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .btn-sm {
    padding: 0.35rem 0.75rem;
    font-size: 0.8rem;
  }

  .btn-subtle {
    background: none !important;
    border: none !important;
    color: var(--text-secondary) !important;
    padding: 0.3rem 0.6rem;
    cursor: pointer;
    font-size: 0.8rem;
    border-radius: 6px;
    transition: all 0.15s;
  }

  .btn-subtle:hover {
    color: var(--accent) !important;
    background: rgba(233, 69, 96, 0.1) !important;
  }

  .btn-subtle-danger:hover {
    color: #f44336 !important;
    background: rgba(244, 67, 54, 0.1) !important;
  }

  .btn-icon-danger {
    background: none !important;
    border: none !important;
    color: var(--text-secondary) !important;
    cursor: pointer;
    font-size: 0.9rem;
    padding: 0.2rem 0.4rem;
    border-radius: 4px;
  }

  .btn-icon-danger:hover {
    color: #f44336 !important;
    background: rgba(244, 67, 54, 0.15) !important;
  }

  .btn-spinner {
    display: inline-block;
    width: 14px;
    height: 14px;
    border: 2px solid var(--border-color);
    border-top: 2px solid var(--accent);
    border-radius: 50%;
    animation: spin 0.6s linear infinite;
  }

  /* ═══ Language Pills ═══ */
  .language-pills {
    display: flex;
    gap: 0.5rem;
  }

  .pill {
    padding: 0.6rem 1.2rem;
    background: var(--bg-tertiary) !important;
    border: 2px solid transparent !important;
    border-radius: 10px;
    cursor: pointer;
    font-size: 0.95rem;
    color: var(--text-primary);
    transition: all 0.15s ease;
  }

  .pill:hover:not(:disabled) {
    border-color: var(--border-color) !important;
    background: var(--bg-hover) !important;
  }

  .pill-active {
    background: var(--accent) !important;
    color: white !important;
    border-color: var(--accent) !important;
  }

  .pill:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  /* ═══ Libraries ═══ */
  .library-grid {
    display: flex;
    flex-direction: column;
    gap: 0.6rem;
  }

  .library-card {
    background: var(--bg-tertiary);
    border: 1px solid transparent;
    border-radius: 10px;
    padding: 0.85rem 1rem;
    transition: border-color 0.15s;
  }

  .library-card:hover {
    border-color: rgba(255, 255, 255, 0.1);
  }

  .library-missing {
    border-color: rgba(255, 193, 7, 0.3);
    opacity: 0.75;
  }

  .library-header {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    margin-bottom: 0.3rem;
  }

  .library-name {
    font-weight: 600;
    font-size: 0.95rem;
    color: var(--text-primary);
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }

  .library-path {
    display: block;
    font-size: 0.78rem;
    color: var(--text-secondary);
    font-family: 'Cascadia Code', 'Fira Code', monospace;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    background: none;
    padding: 0;
    margin-bottom: 0.5rem;
  }

  .library-footer {
    display: flex;
    gap: 0.25rem;
  }

  /* ═══ Badges ═══ */
  .badge {
    font-size: 0.7rem;
    font-weight: 600;
    padding: 0.1rem 0.45rem;
    border-radius: 5px;
    text-transform: uppercase;
    letter-spacing: 0.02em;
    white-space: nowrap;
  }

  .badge-accent {
    background: var(--accent);
    color: white;
  }

  .badge-warning {
    background: rgba(255, 193, 7, 0.2);
    color: var(--warning);
  }

  /* ═══ Tools ═══ */
  .tools-row {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  .tool-chip {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    padding: 0.7rem 1rem;
    background: var(--bg-tertiary);
    border-radius: 10px;
  }

  .tool-icon { font-size: 1.2rem; }

  .tool-details {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 0.1rem;
    min-width: 0;
  }

  .tool-label {
    font-weight: 600;
    font-size: 0.9rem;
    color: var(--text-primary);
  }

  .tool-meta {
    font-size: 0.78rem;
    color: var(--text-secondary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .status-dot {
    width: 10px;
    height: 10px;
    border-radius: 50%;
    flex-shrink: 0;
  }

  .status-green { background: #4caf50; box-shadow: 0 0 6px rgba(76, 175, 80, 0.4); }
  .status-yellow { background: #ff9800; box-shadow: 0 0 6px rgba(255, 152, 0, 0.4); }
  .status-red { background: #f44336; box-shadow: 0 0 6px rgba(244, 67, 54, 0.4); }

  /* ═══ Toggle Switch ═══ */
  .toggle-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
    padding: 0.6rem 0.5rem;
    border-radius: 8px;
    cursor: pointer;
    transition: background 0.15s;
  }

  .toggle-row:hover {
    background: rgba(255, 255, 255, 0.03);
  }

  .toggle-info {
    display: flex;
    flex-direction: column;
    gap: 0.15rem;
    flex: 1;
  }

  .toggle-label {
    font-size: 0.9rem;
    font-weight: 500;
    color: var(--text-primary);
  }

  .toggle-hint {
    font-size: 0.78rem;
    color: var(--text-secondary);
    line-height: 1.4;
  }

  .toggle-switch {
    width: 44px;
    height: 24px;
    background: var(--bg-tertiary);
    border: 2px solid var(--border-color);
    border-radius: 12px;
    position: relative;
    transition: all 0.2s ease;
    flex-shrink: 0;
    cursor: pointer;
  }

  .toggle-on {
    background: var(--accent);
    border-color: var(--accent);
  }

  .toggle-knob {
    width: 18px;
    height: 18px;
    background: white;
    border-radius: 50%;
    position: absolute;
    top: 1px;
    left: 1px;
    transition: transform 0.2s ease;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.3);
  }

  .toggle-on .toggle-knob {
    transform: translateX(20px);
  }

  /* ═══ Radio Cards ═══ */
  .setting-group {
    display: flex;
    flex-direction: column;
    gap: 0.6rem;
  }

  .group-label {
    font-size: 0.85rem;
    font-weight: 500;
    color: var(--text-secondary);
  }

  .radio-cards {
    display: flex;
    gap: 0.5rem;
  }

  .radio-card {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 0.4rem;
    padding: 0.75rem 0.5rem;
    background: var(--bg-tertiary) !important;
    border: 2px solid transparent !important;
    border-radius: 10px;
    cursor: pointer;
    transition: all 0.15s ease;
    text-align: center;
  }

  .radio-card:hover:not(:disabled) {
    border-color: var(--border-color) !important;
    background: var(--bg-hover) !important;
  }

  .radio-active {
    border-color: var(--accent) !important;
    background: rgba(233, 69, 96, 0.1) !important;
  }

  .radio-card:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .radio-icon {
    font-size: 1.3rem;
  }

  .radio-label {
    font-size: 0.8rem;
    font-weight: 500;
    color: var(--text-primary);
    line-height: 1.3;
  }

  .radio-hint {
    font-size: 0.7rem;
    color: var(--text-secondary);
    line-height: 1.2;
  }

  /* ═══ Divider ═══ */
  .divider {
    height: 1px;
    background: rgba(255, 255, 255, 0.06);
    margin: 0.25rem 0;
  }

  /* ═══ Sub-setting (indented) ═══ */
  .sub-setting {
    padding-left: 1rem;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  .sub-label {
    font-size: 0.85rem;
    color: var(--text-secondary);
  }

  .folder-picker {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    flex-wrap: wrap;
  }

  .folder-path {
    font-size: 0.78rem;
    background: var(--bg-tertiary);
    padding: 0.35rem 0.7rem;
    border-radius: 6px;
    color: var(--text-primary);
    font-family: 'Cascadia Code', 'Fira Code', monospace;
    word-break: break-all;
  }

  .text-muted {
    color: var(--text-secondary);
    font-size: 0.85rem;
    font-style: italic;
  }

  /* ═══ Paths ═══ */
  .paths-grid {
    display: flex;
    flex-direction: column;
    gap: 0.6rem;
  }

  .path-row {
    display: flex;
    flex-direction: column;
    gap: 0.2rem;
  }

  .path-label {
    font-size: 0.8rem;
    color: var(--text-secondary);
    font-weight: 500;
  }

  .path-value {
    font-size: 0.78rem;
    background: var(--bg-tertiary);
    padding: 0.4rem 0.7rem;
    border-radius: 6px;
    color: var(--text-primary);
    font-family: 'Cascadia Code', 'Fira Code', monospace;
    word-break: break-all;
  }

  /* ═══ Dev Card ═══ */
  .dev-card {
    border-style: dashed;
    border-color: rgba(255, 255, 255, 0.08);
    opacity: 0.85;
  }

  .dev-card:hover {
    opacity: 1;
  }

  /* ═══ Empty State ═══ */
  .empty-state {
    text-align: center;
    padding: 1.5rem;
    color: var(--text-secondary);
  }

  .empty-icon {
    font-size: 2rem;
    display: block;
    margin-bottom: 0.5rem;
  }

  .empty-hint {
    font-size: 0.85rem;
    margin-top: 0.3rem;
    opacity: 0.7;
  }

  /* ═══ About Section ═══ */
  .about-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0.5rem 0;
  }

  .about-row + .about-row {
    border-top: 1px solid var(--border-color);
    padding-top: 0.75rem;
  }

  .about-label {
    color: var(--text-secondary);
    font-size: 0.9rem;
  }

  .about-value {
    color: var(--text-primary);
    font-weight: 600;
    font-family: monospace;
  }

  .btn-update {
    background: var(--accent);
    color: white;
    border: none;
    padding: 0.5rem 1.25rem;
    border-radius: 6px;
    cursor: pointer;
    font-size: 0.85rem;
    font-weight: 500;
    width: 100%;
    transition: filter 0.15s;
  }

  .btn-update:hover:not(:disabled) {
    filter: brightness(1.15);
  }

  .btn-update:disabled {
    opacity: 0.7;
    cursor: not-allowed;
  }
</style>

