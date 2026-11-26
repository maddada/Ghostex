// Desktop audio: the worker decodes Opus and posts PCM straight to the player worklet
// over a MessageChannel, so audio never waits on the main thread.
import type { App } from './app';

export function createAudio(app: App) {
	let started = false;
	let ac: AudioContext | null = null;

	/** Starts (or resumes) playback; browsers only allow it after a user gesture. */
	async function start() {
		if (ac && ac.state === 'suspended') {
			ac.resume().catch(() => {});
			return;
		}
		if (started || typeof AudioDecoder === 'undefined') return;
		started = true;
		try {
			ac = new AudioContext({ latencyHint: 'interactive', sampleRate: 48000 });
			await ac.audioWorklet.addModule('audio.js');
			const node = new AudioWorkletNode(ac, 'pcm-player', {
				outputChannelCount: [2],
			});
			node.connect(ac.destination);
			const ch = new MessageChannel();
			node.port.postMessage({ port: ch.port2 }, [ch.port2]);
			app.post({ t: 'audioPort', port: ch.port1 }, [ch.port1]);
			if (ac.state !== 'running') ac.resume().catch(() => {});
		} catch (err) {
			console.warn('[voxl] audio unavailable', err);
		}
	}
	return { start };
}
