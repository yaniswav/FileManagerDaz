import { check } from '@tauri-apps/plugin-updater';
import { relaunch } from '@tauri-apps/plugin-process';
import { addToast } from '$lib/stores/toast.svelte';

let checking = $state(false);
let downloading = $state(false);
let progress = $state(0);

export const updaterState = {
	get checking() { return checking; },
	get downloading() { return downloading; },
	get progress() { return progress; },
};

/**
 * Check for updates and prompt the user.
 * @param silent If true, don't show a toast when already up-to-date.
 */
export async function checkForUpdates(silent = false): Promise<void> {
	if (checking || downloading) return;
	checking = true;

	try {
		const update = await check();

		if (!update) {
			if (!silent) addToast('You are on the latest version.', 'success');
			return;
		}

		addToast(
			`Update ${update.version} available! Downloading...`,
			'info',
			'Update Available',
			0,
		);

		downloading = true;
		progress = 0;

		let totalBytes = 0;
		let downloadedBytes = 0;

		await update.downloadAndInstall((event) => {
			if (event.event === 'Started' && event.data.contentLength) {
				totalBytes = event.data.contentLength;
			} else if (event.event === 'Progress') {
				downloadedBytes += event.data.chunkLength;
				progress = totalBytes > 0 ? Math.round((downloadedBytes / totalBytes) * 100) : 0;
			} else if (event.event === 'Finished') {
				progress = 100;
			}
		});

		addToast('Update installed! Restarting...', 'success', 'Update Complete');

		// Short delay so the user sees the toast
		await new Promise((r) => setTimeout(r, 1500));
		await relaunch();
	} catch (e) {
		if (!silent) {
			const msg = e instanceof Error ? e.message : String(e);
			const isNetworkError = msg.includes('endpoint') || msg.includes('fetch') || msg.includes('JSON');
			const userMsg = isNetworkError
				? 'Could not reach the update server. Check your connection or try again later.'
				: `Update check failed: ${msg}`;
			addToast(userMsg, 'error');
		}
	} finally {
		checking = false;
		downloading = false;
		progress = 0;
	}
}
