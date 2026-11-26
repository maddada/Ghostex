// Microphone: capture -> Opus (WebCodecs) -> server, which feeds the VM's microphone.
import { Bin, cmd } from '../protocol';
import type { App } from './app';
import { el } from './app';

const RATE = 48000;

export function createMic(app: App) {
	const btn = el<HTMLButtonElement>('mic-btn');
	let on = false;
	let stream: MediaStream | null = null;
	let ctx: AudioContext | null = null;
	let enc: AudioEncoder | null = null;
	let ts = 0;

	function show() {
		btn.classList.toggle('on', on);
		btn.setAttribute('aria-pressed', String(on));
		if (!btn.classList.contains('err'))
			btn.title = on ? 'Turn microphone off' : 'Turn microphone on';
		// While the mic is live the toolbar stays up as a recording indicator.
		app.tools.classList.toggle('mic-on', on);
	}

	function setup(enabled: boolean) {
		const available = enabled && !!navigator.mediaDevices?.getUserMedia;
		app.tools.classList.toggle('mic', available);
		if (!available) stop();
	}

	async function opusEncoder(): Promise<AudioEncoder | null> {
		if (typeof AudioEncoder === 'undefined') return null;
		const base: AudioEncoderConfig = {
			codec: 'opus',
			sampleRate: RATE,
			numberOfChannels: 1,
			bitrate: 32000,
		};
		const voip = {
			...base,
			opus: { frameDuration: 20000, application: 'voip' },
		} as AudioEncoderConfig;
		// Try the VoIP-tuned config first, then the plain one, in order.
		for (const config of [voip, base]) {
			try {
				// oxlint-disable-next-line no-await-in-loop
				if (!(await AudioEncoder.isConfigSupported(config)).supported) continue;
				const e = new AudioEncoder({
					output: (chunk) => {
						const b = new Uint8Array(chunk.byteLength);
						chunk.copyTo(b);
						app.sendBin(Bin.MicOpus, b);
					},
					error: (err) => console.warn('[voxl] microphone encoder', err),
				});
				e.configure(config);
				return e;
			} catch {}
		}
		return null;
	}

	function onChunk(f: Float32Array) {
		if (!on) return;
		if (enc && enc.state === 'configured') {
			const data = new AudioData({
				format: 'f32-planar',
				sampleRate: RATE,
				numberOfFrames: f.length,
				numberOfChannels: 1,
				timestamp: ts,
				data: f,
			});
			ts += 20000;
			enc.encode(data);
			data.close();
		} else {
			// No WebCodecs audio encoder (older Safari/Firefox): send 16-bit PCM.
			const pcm = new Int16Array(f.length);
			for (let i = 0; i < f.length; i++)
				pcm[i] = Math.max(-1, Math.min(1, f[i])) * 32767;
			app.sendBin(Bin.MicPcm, new Uint8Array(pcm.buffer));
		}
	}

	async function start() {
		btn.classList.remove('err');
		try {
			stream = await navigator.mediaDevices.getUserMedia({
				audio: {
					channelCount: 1,
					echoCancellation: true,
					noiseSuppression: true,
					autoGainControl: true,
				},
			});
			ctx = new AudioContext({ sampleRate: RATE, latencyHint: 'interactive' });
			await ctx.audioWorklet.addModule('mic.js');
			enc = await opusEncoder();
			const node = new AudioWorkletNode(ctx, 'mic-capture', {
				numberOfInputs: 1,
				numberOfOutputs: 0,
				channelCount: 1,
				channelCountMode: 'explicit',
			});
			node.port.addEventListener('message', (e: MessageEvent<Float32Array>) =>
				onChunk(e.data)
			);
			node.port.start();
			ctx.createMediaStreamSource(stream).connect(node);
			if (ctx.state !== 'running') await ctx.resume();
			on = true;
			app.send(cmd.mic(true));
		} catch (err) {
			console.warn('[voxl] microphone unavailable', err);
			stop();
			btn.classList.add('err');
			btn.title =
				err instanceof DOMException && err.name === 'NotAllowedError'
					? 'Microphone access was blocked'
					: 'No microphone available';
		}
		show();
	}

	function stop() {
		if (on) app.send(cmd.mic(false));
		on = false;
		stream?.getTracks().forEach((t) => t.stop());
		ctx?.close().catch(() => {});
		if (enc && enc.state !== 'closed') {
			try {
				enc.close();
			} catch {}
		}
		stream = null;
		ctx = null;
		enc = null;
		show();
	}

	btn.addEventListener('click', () => (on ? stop() : start()));
	return { setup, stop };
}
