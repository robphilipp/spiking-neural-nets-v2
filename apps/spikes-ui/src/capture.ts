// Capturing the microphone and the camera in the browser (app plan 05, phase I4): the thin DOM and media glue around
// the pure channel code in realtimeChannels.ts. A capture is opened (asking for the device), then started with a
// function that gets each batch of samples; the run's sensors and the Inputs page's previews both use it.

import { bandLevels, cameraChannels, frameChanges, lumaGrid } from "./realtimeChannels";
import type { RealtimeSource } from "./inputFile";

/** A batch of samples: the capture's clock at the first sample (ns from `start`), the spacing, and the values
 *  (sample-major: all channels of a sample, then the next). */
export type Emit = (t0Ns: bigint, dtNs: number, values: Float32Array) => void;

export interface Capture {
  /** Values per sample. */
  readonly channels: number;
  /** Begins calling `emit` with batches, until `stop`. */
  start(emit: Emit): void;
  /** Stops capturing and releases the device. */
  stop(): void;
}

/** Opens a capture for a microphone or camera source sending `channels` channels (see `channelPlan`). Rejects with a
 *  message fit for the user if the device isn't there or access is refused. */
export type OpenCapture = (source: RealtimeSource, channels: number) => Promise<Capture>;

/** A microphone's batches: one spectrum sample every 10 ms. */
const AUDIO_MS = 10;
/** A camera's frames are averaged over cells from a canvas this many times the grid's size (a smooth downsample
 *  that doesn't depend on the browser's scaling). */
const OVERSAMPLE = 4;

function explain(e: unknown, device: string): Error {
  const name = e instanceof DOMException ? e.name : "";
  if (name === "NotAllowedError" || name === "SecurityError") return new Error(`access to the ${device} was refused: allow it for this app and try again`);
  if (name === "NotFoundError" || name === "OverconstrainedError") return new Error(`no ${device} found`);
  if (name === "NotReadableError") return new Error(`the ${device} is in use by another program`);
  return e instanceof Error ? e : new Error(String(e));
}

function media(): MediaDevices {
  if (!navigator.mediaDevices?.getUserMedia) throw new Error("this browser can't reach the microphone or camera here (it needs https or localhost)");
  return navigator.mediaDevices;
}

async function openMicrophone(channels: number): Promise<Capture> {
  let stream: MediaStream;
  try {
    // The raw sound: echo cancellation, noise suppression and gain control would reshape it (noise suppression
    // removes steady tones altogether).
    stream = await media().getUserMedia({ audio: { echoCancellation: false, noiseSuppression: false, autoGainControl: false } });
  } catch (e) {
    throw explain(e, "microphone");
  }
  const audio = new AudioContext();
  const analyser = audio.createAnalyser();
  analyser.fftSize = 2048;
  audio.createMediaStreamSource(stream).connect(analyser);
  void audio.resume().catch(() => {});
  const spectrum = new Float32Array(analyser.frequencyBinCount);
  let timer: ReturnType<typeof setInterval> | undefined;
  return {
    channels,
    start(emit) {
      const begin = performance.now();
      let sent = 0; // samples
      timer = setInterval(() => {
        const due = Math.floor((performance.now() - begin) / AUDIO_MS);
        if (due <= sent) return;
        analyser.getFloatFrequencyData(spectrum);
        const bands = bandLevels(spectrum, channels, audio.sampleRate);
        const values = new Float32Array((due - sent) * channels);
        for (let s = 0; s < due - sent; s++) values.set(bands, s * channels);
        emit(BigInt(sent * AUDIO_MS * 1e6), AUDIO_MS * 1e6, values);
        sent = due;
      }, AUDIO_MS);
    },
    stop() {
      clearInterval(timer);
      stream.getTracks().forEach((t) => t.stop());
      void audio.close().catch(() => {});
    },
  };
}

async function openCamera(source: Extract<RealtimeSource, { kind: "camera" }>): Promise<Capture> {
  let stream: MediaStream;
  try {
    stream = await media().getUserMedia({ video: { width: { ideal: 320 }, height: { ideal: 240 }, frameRate: { ideal: 30 } } });
  } catch (e) {
    throw explain(e, "camera");
  }
  const video = document.createElement("video");
  video.muted = true;
  video.playsInline = true;
  video.srcObject = stream;
  try {
    await video.play();
  } catch (e) {
    stream.getTracks().forEach((t) => t.stop());
    throw explain(e, "camera");
  }
  const { grid, mode, level } = source;
  const canvas = document.createElement("canvas");
  canvas.width = grid.cols * OVERSAMPLE;
  canvas.height = grid.rows * OVERSAMPLE;
  const ctx = canvas.getContext("2d", { willReadFrequently: true })!;
  let stopped = false;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let callback: number | undefined;
  return {
    channels: cameraChannels(grid, mode),
    start(emit) {
      const begin = performance.now();
      let previous: Float32Array | null = null;
      let lastMs = begin;
      let interval = 1000 / 30;
      const frame = () => {
        if (stopped) return;
        const now = performance.now();
        if (video.videoWidth > 0 && now > lastMs) {
          ctx.drawImage(video, 0, 0, canvas.width, canvas.height);
          const cells = lumaGrid(ctx.getImageData(0, 0, canvas.width, canvas.height).data, canvas.width, canvas.height, grid);
          // Spacing: how long since the last frame (the first frame has no earlier one to measure against).
          if (previous) interval = now - lastMs;
          lastMs = now;
          if (mode === "luminance") emit(BigInt(Math.round((now - begin) * 1e6)), interval * 1e6, cells);
          else if (previous) emit(BigInt(Math.round((now - begin) * 1e6)), interval * 1e6, frameChanges(previous, cells, level));
          previous = cells;
        }
        next();
      };
      const next = () => {
        if (stopped) return;
        if ("requestVideoFrameCallback" in video) callback = video.requestVideoFrameCallback(frame);
        else timer = setTimeout(frame, 1000 / 30);
      };
      next();
    },
    stop() {
      stopped = true;
      clearTimeout(timer);
      if (callback !== undefined && "cancelVideoFrameCallback" in video) video.cancelVideoFrameCallback(callback);
      stream.getTracks().forEach((t) => t.stop());
      video.srcObject = null;
    },
  };
}

export const openCapture: OpenCapture = (source, channels) => {
  switch (source.kind) {
    case "microphone":
      return openMicrophone(channels);
    case "camera":
      return openCamera(source);
    case "sensor":
      return Promise.reject(new Error("a sensor source is fed by a program, not captured here"));
  }
};
