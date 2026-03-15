<script lang="ts">
  import { t } from '$lib/i18n';
  import { hideToTray, quitApp, setCloseAction } from '$lib/api/commands';

  let { visible = $bindable(false) } : { visible: boolean } = $props();
  let rememberChoice = $state(false);

  async function handleMinimize() {
    if (rememberChoice) {
      await setCloseAction('minimize');
    }
    visible = false;
    await hideToTray();
  }

  async function handleQuit() {
    if (rememberChoice) {
      await setCloseAction('quit');
    }
    visible = false;
    await quitApp();
  }

  function handleCancel() {
    visible = false;
  }
</script>

{#if visible}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="overlay" onclick={handleCancel}>
    <div class="dialog" onclick={(e) => e.stopPropagation()}>
      <div class="dialog-icon">🖥️</div>
      <h2>{$t('closeDialog.title')}</h2>
      <p class="dialog-message">{$t('closeDialog.message')}</p>

      <div class="dialog-actions">
        <button class="btn-minimize" onclick={handleMinimize}>
          <span class="btn-icon">📥</span>
          <span class="btn-content">
            <span class="btn-label">{$t('closeDialog.minimize')}</span>
            <span class="btn-hint">{$t('closeDialog.minimizeHint')}</span>
          </span>
        </button>
        <button class="btn-quit" onclick={handleQuit}>
          <span class="btn-icon">🚪</span>
          <span class="btn-content">
            <span class="btn-label">{$t('closeDialog.quit')}</span>
            <span class="btn-hint">{$t('closeDialog.quitHint')}</span>
          </span>
        </button>
      </div>

      <label class="remember-checkbox">
        <input type="checkbox" bind:checked={rememberChoice} />
        <span>{$t('closeDialog.remember')}</span>
      </label>

      <button class="btn-cancel" onclick={handleCancel}>
        {$t('closeDialog.cancel')}
      </button>
    </div>
  </div>
{/if}

<style>
  .overlay {
    position: fixed;
    top: 0;
    left: 0;
    width: 100%;
    height: 100%;
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

  .dialog {
    background: var(--bg-secondary);
    border: 1px solid var(--border-color);
    border-radius: 16px;
    padding: 2rem;
    width: 420px;
    max-width: 90vw;
    text-align: center;
    box-shadow: 0 20px 60px rgba(0, 0, 0, 0.5);
    animation: slideUp 0.2s ease;
  }

  @keyframes slideUp {
    from { transform: translateY(20px); opacity: 0; }
    to { transform: translateY(0); opacity: 1; }
  }

  .dialog-icon {
    font-size: 2.5rem;
    margin-bottom: 0.75rem;
  }

  .dialog h2 {
    margin: 0 0 0.5rem;
    font-size: 1.25rem;
    color: var(--text-primary);
  }

  .dialog-message {
    color: var(--text-secondary);
    font-size: 0.9rem;
    margin: 0 0 1.5rem;
    line-height: 1.5;
  }

  .dialog-actions {
    display: flex;
    gap: 0.75rem;
    margin-bottom: 1.25rem;
  }

  .dialog-actions button {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 0.5rem;
    padding: 1rem;
    border: 2px solid var(--border-color);
    border-radius: 12px;
    background: var(--bg-tertiary);
    color: var(--text-primary);
    cursor: pointer;
    transition: all 0.15s ease;
    text-align: center;
  }

  .dialog-actions button:hover {
    border-color: var(--accent);
    background: rgba(233, 69, 96, 0.1);
  }

  .btn-icon {
    font-size: 1.5rem;
    flex-shrink: 0;
  }

  .btn-content {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 0.15rem;
  }

  .btn-label {
    font-weight: 600;
    font-size: 0.95rem;
  }

  .btn-hint {
    font-size: 0.75rem;
    color: var(--text-secondary);
    font-weight: normal;
  }

  .remember-checkbox {
    display: inline-flex;
    align-items: center;
    gap: 0.5rem;
    font-size: 0.85rem;
    color: var(--text-secondary);
    cursor: pointer;
    margin-bottom: 1rem;
    user-select: none;
  }

  .remember-checkbox input {
    width: 16px;
    height: 16px;
    accent-color: var(--accent);
    cursor: pointer;
  }

  .btn-cancel {
    background: none !important;
    border: none !important;
    color: var(--text-secondary) !important;
    font-size: 0.85rem;
    cursor: pointer;
    padding: 0.4rem 1rem;
  }

  .btn-cancel:hover {
    color: var(--text-primary) !important;
    background: none !important;
  }
</style>
