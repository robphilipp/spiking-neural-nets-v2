"""Route a microphone into a spiking network, like a cochlea.

Audio (16 kHz) → every 1 ms a 256-sample FFT → 64 log-spaced bands (100 Hz–8 kHz) → log-compressed, adaptively
normalised to 0..1 → rate-coded input to 64 neurons, sent in 10 ms batches stamped with the audio clock (so the
engine keeps the exact 1 ms spacing whatever the network jitter).

    pip install -e 'clients/python[microphone]'
    spikes-server &
    python clients/python/examples/microphone.py                     # creates a 64-neuron "cochlea" session
    python clients/python/examples/microphone.py --session 3 --prefix ear-   # drives an existing session
    python clients/python/examples/microphone.py --synthetic         # a generated chirp instead of the microphone

macOS asks for microphone permission the first time.
"""

from __future__ import annotations

import argparse
import queue
import time

import numpy as np

from spikes_sensor import SensorClient

RATE = 16_000
BLOCK = RATE // 100  # 10 ms
HOP = RATE // 1000  # 1 ms
WINDOW = 256


def band_matrix(channels: int, lo: float = 100.0, hi: float = 8000.0) -> np.ndarray:
    """(channels × FFT bins) weights averaging the bins of each log-spaced band."""
    freqs = np.fft.rfftfreq(WINDOW, 1 / RATE)
    edges = np.geomspace(lo, hi, channels + 1)
    m = np.zeros((channels, len(freqs)), dtype=np.float32)
    for c in range(channels):
        inside = (freqs >= edges[c]) & (freqs < edges[c + 1])
        if not inside.any():  # narrow low bands: take the nearest bin
            inside = np.abs(freqs - (edges[c] + edges[c + 1]) / 2) == np.abs(freqs - (edges[c] + edges[c + 1]) / 2).min()
        m[c, inside] = 1.0 / inside.sum()
    return m


class Cochlea:
    """Turns 10 ms audio blocks into 10 × channels band envelopes in 0..1."""

    def __init__(self, channels: int):
        self.bands = band_matrix(channels)
        self.window = np.hanning(WINDOW).astype(np.float32)
        self.history = np.zeros(WINDOW - HOP, dtype=np.float32)
        self.floor, self.ceiling = -6.0, -2.0  # adaptive range of log10 power

    def __call__(self, block: np.ndarray) -> np.ndarray:
        audio = np.concatenate([self.history, block])
        self.history = audio[-(WINDOW - HOP):]
        frames = np.lib.stride_tricks.sliding_window_view(audio, WINDOW)[::HOP][: BLOCK // HOP]
        power = np.abs(np.fft.rfft(frames * self.window, axis=1)) ** 2
        level = np.log10(power @ self.bands.T + 1e-12)
        # Slowly track the loudest and quietest levels, so the 0..1 range follows the room.
        self.ceiling = max(self.ceiling * 0.999 + level.max() * 0.001, level.max())
        self.floor = min(self.floor * 0.999 + np.median(level) * 0.001, np.median(level))
        return np.clip((level - self.floor) / max(self.ceiling - self.floor, 1e-3), 0.0, 1.0).astype(np.float32)


def chirp_blocks(seconds: float):
    """A tone gliding 200 Hz → 6 kHz once a second, in 10 ms blocks."""
    t = np.arange(int(seconds * RATE)) / RATE
    freq = 200 * (30 ** (t % 1.0))
    signal = 0.3 * np.sin(2 * np.pi * np.cumsum(freq) / RATE).astype(np.float32)
    for i in range(0, len(signal) - BLOCK + 1, BLOCK):
        yield signal[i : i + BLOCK]


def microphone_blocks(seconds: float):
    import sounddevice as sd

    blocks: queue.Queue[np.ndarray] = queue.Queue()
    with sd.InputStream(samplerate=RATE, channels=1, blocksize=BLOCK, dtype="float32",
                        callback=lambda data, *_: blocks.put(data[:, 0].copy())):
        end = time.monotonic() + seconds
        while time.monotonic() < end:
            yield blocks.get()


def main() -> None:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("--server", default="ws://127.0.0.1:7070")
    p.add_argument("--token")
    p.add_argument("--session", type=int, help="an existing session (default: create a cochlea network)")
    p.add_argument("--prefix", default="cochlea-", help="names of the neurons to drive, one per band")
    p.add_argument("--channels", type=int, default=64)
    p.add_argument("--seconds", type=float, default=10.0)
    p.add_argument("--max-hz", type=float, default=150.0, help="firing rate at full loudness")
    p.add_argument("--synthetic", action="store_true", help="a generated chirp instead of the microphone")
    args = p.parse_args()

    with SensorClient(args.server, args.token) as client:
        session = args.session
        if session is None:
            neurons = ", ".join(
                f"(nid={args.prefix}{i}, mst=1 mV, mpn=0 mV, LOC=(cst=ct, px1={i * 10}, px2=0, px3=0))"
                for i in range(args.channels)
            )
            session = client.create_session("cochlea", f"(NRN=[{neurons}])")
            client.build(session)
            client.start(session, speed=1.0)
        targets = client.neurons_named(session, args.prefix)[: args.channels]
        if len(targets) < args.channels:
            raise SystemExit(f"only {len(targets)} neurons start with '{args.prefix}'")
        sensor = client.connect_sensor(session, "microphone", [[n] for n in targets],
                                       {"type": "rate", "maxHz": args.max_hz, "mv": 2.0})
        cochlea = Cochlea(args.channels)
        blocks = chirp_blocks(args.seconds) if args.synthetic else microphone_blocks(args.seconds)
        start, sent = time.monotonic(), 0
        for i, block in enumerate(blocks):
            envelopes = cochlea(block)  # (10, channels)
            client.send_samples(sensor, i * 10_000_000, 1_000_000, args.channels, envelopes.ravel())
            sent += 1
            if args.synthetic:  # pace a generated signal like a real one
                time.sleep(max(0.0, start + sent * 0.01 - time.monotonic()))
        spikes = [client.query_neuron(session, n)["spikes"] for n in targets]
        print(f"sent {sent} batches ({sent * 10} ms of audio); spikes per band: {spikes}")
        if args.session is None:
            client.stop(session)


if __name__ == "__main__":
    main()
