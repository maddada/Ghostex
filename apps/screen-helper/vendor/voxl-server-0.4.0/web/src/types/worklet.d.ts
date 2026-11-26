// AudioWorkletGlobalScope, which TypeScript's libraries don't include.
declare abstract class AudioWorkletProcessor {
	readonly port: MessagePort;
	constructor();
	abstract process(
		inputs: Float32Array[][],
		outputs: Float32Array[][],
		parameters: Record<string, Float32Array>
	): boolean;
}
declare function registerProcessor(
	name: string,
	ctor: new () => AudioWorkletProcessor
): void;
