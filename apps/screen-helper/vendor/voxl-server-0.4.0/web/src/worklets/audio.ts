// Jitter-buffered PCM player. Receives planar Float32 [L, R] chunks on a MessagePort.
import type { PcmChunk } from '../protocol';

class PcmPlayer extends AudioWorkletProcessor {
	private cap = 48000; // 1 s ring
	private l = new Float32Array(this.cap);
	private r = new Float32Array(this.cap);
	private rd = 0;
	private wr = 0;
	private fill = 0;
	private target = 2400; // 50 ms
	private priming = true;

	constructor() {
		super();
		this.port.addEventListener(
			'message',
			(e: MessageEvent<{ port?: MessagePort }>) => {
				const port = e.data.port;
				if (!port) return;
				port.addEventListener('message', (m: MessageEvent<PcmChunk>) =>
					this.push(m.data)
				);
				// Unlike assigning onmessage, addEventListener doesn't start a port.
				port.start();
			}
		);
		this.port.start();
	}

	private push({ l, r }: PcmChunk) {
		for (let i = 0; i < l.length; i++) {
			this.l[this.wr] = l[i];
			this.r[this.wr] = r ? r[i] : l[i];
			this.wr = (this.wr + 1) % this.cap;
		}
		this.fill += l.length;
		// Too much queued: drop the oldest so latency stays bounded.
		if (this.fill > this.target * 3) {
			const drop = this.fill - this.target;
			this.rd = (this.rd + drop) % this.cap;
			this.fill -= drop;
		}
	}

	process(_: Float32Array[][], outputs: Float32Array[][]) {
		const out = outputs[0];
		const n = out[0].length;
		if (this.priming) {
			if (this.fill < this.target) return true;
			this.priming = false;
		}
		if (this.fill < n) {
			this.priming = true;
			return true;
		}
		for (let i = 0; i < n; i++) {
			out[0][i] = this.l[this.rd];
			if (out[1]) out[1][i] = this.r[this.rd];
			this.rd = (this.rd + 1) % this.cap;
		}
		this.fill -= n;
		return true;
	}
}
registerProcessor('pcm-player', PcmPlayer);
