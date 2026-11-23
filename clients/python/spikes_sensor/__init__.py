"""Stream sensor data into a spikes-server session.

    from spikes_sensor import SensorClient

    with SensorClient("ws://127.0.0.1:7070") as client:
        targets = client.neurons_named(session, "cochlea-")
        sensor = client.connect_sensor(session, "microphone", [[n] for n in targets],
                                       {"type": "rate", "maxHz": 200.0, "mv": 3.0})
        client.send_samples(sensor, t0_ns, 1_000_000, channels=64, values=values)  # sample-major

The wire formats are described in crates/spikes-api (protocol.rs for JSON, input.rs for sample batches).
"""

from __future__ import annotations

import json
import struct
from typing import Any, Iterable, Sequence
from urllib.parse import urlencode

from websockets.exceptions import ConnectionClosed
from websockets.sync.client import connect

PROTOCOL_VERSION = 1
_HEADER = struct.Struct("<4sHHIIQHHI")  # magic, version, flags, sensor, seq, t0_ns, channels, count, dt_ns


class SpikesError(Exception):
    """A request the engine refused. `code` is e.g. 'parse', 'invalidState', 'notFound'."""

    def __init__(self, error: dict[str, Any]):
        super().__init__(error.get("message", "error"))
        self.code = error.get("code")
        self.error = error


def encode_samples(sensor: int, seq: int, t0_ns: int, dt_ns: int, channels: int, values: Sequence[float]) -> bytes:
    """A sample batch: `values` is sample-major (values[s * channels + c])."""
    if channels <= 0 or len(values) % channels:
        raise ValueError(f"{len(values)} values aren't whole samples of {channels} channels")
    count = len(values) // channels
    header = _HEADER.pack(b"SPKS", PROTOCOL_VERSION, 0, sensor, seq & 0xFFFFFFFF, t0_ns, channels, count, dt_ns)
    try:  # numpy arrays: one fast conversion
        import numpy as np

        payload = np.asarray(values, dtype="<f4").tobytes()
    except ImportError:
        payload = struct.pack(f"<{len(values)}f", *values)
    return header + payload


def encode_events(sensor: int, seq: int, t0_ns: int, events: Iterable[tuple[int, int]]) -> bytes:
    """An event batch: (channel, offset ns from t0_ns) per event."""
    events = list(events)
    count = len(events)
    header = _HEADER.pack(b"SPKS", PROTOCOL_VERSION, 1, sensor, seq & 0xFFFFFFFF, t0_ns, 0, count, 0)
    channels = struct.pack(f"<{count}H", *(c for c, _ in events))
    channels += b"\0" * (-len(channels) % 4)
    offsets = struct.pack(f"<{count}I", *(o for _, o in events))
    return header + channels + offsets


class SensorClient:
    """A blocking connection to a spikes-server for sending sensor data (and making requests)."""

    def __init__(self, server: str = "ws://127.0.0.1:7070", token: str | None = None, timeout: float = 10.0):
        base = server.rstrip("/").replace("http", "ws", 1)
        params = {"v": PROTOCOL_VERSION}
        if token:
            params["token"] = token
        self._connection = connect(f"{base}/api/ws?{urlencode(params)}", max_size=None)
        self._socket = self._connection.__enter__()
        self._timeout = timeout
        self._next_id = 1
        self._seq = 0

    def __enter__(self) -> "SensorClient":
        return self

    def __exit__(self, *exc: object) -> None:
        self.close()

    def close(self) -> None:
        """Closes the connection; the engine disconnects this client's sensors."""
        self._connection.__exit__(None, None, None)

    def _other(self, message: str | bytes) -> None:
        """A message that isn't the answer we're waiting for: errors without a request are raised."""
        if isinstance(message, str):
            m = json.loads(message)
            if m.get("type") == "response" and m.get("id") == 0 and "error" in m["result"]:
                raise SpikesError(m["result"]["error"])

    def request(self, request: dict[str, Any]) -> dict[str, Any]:
        """Sends a request (a dict with a 'type', as in protocol.rs) and returns the response's value."""
        rid = self._next_id
        self._next_id += 1
        self._socket.send(json.dumps({"id": rid, "request": request}))
        while True:
            message = self._socket.recv(timeout=self._timeout)
            if isinstance(message, str):
                m = json.loads(message)
                if m.get("type") == "response" and m.get("id") == rid:
                    if "ok" in m["result"]:
                        return m["result"]["ok"]
                    raise SpikesError(m["result"]["error"])
            self._other(message)

    def create_session(self, name: str, description: str) -> int:
        return self.request({"type": "createSession", "name": name, "description": description})["session"]["id"]

    def build(self, session: int, **settings: Any) -> None:
        self.request({"type": "build", "session": session, "settings": settings})

    def start(self, session: int, speed: float | None = 1.0, duration_ms: float | None = None, record: bool = False) -> None:
        self.request({"type": "start", "session": session, "speed": speed, "durationMs": duration_ms, "record": record})

    def stop(self, session: int) -> None:
        self.request({"type": "stop", "session": session})

    def query_neuron(self, session: int, neuron: int) -> dict[str, Any]:
        return self.request({"type": "queryNeuron", "session": session, "neuron": neuron})["neuron"]

    def neurons_named(self, session: int, prefix: str) -> list[int]:
        """Indices of the neurons whose names start with `prefix`."""
        network = self.request({"type": "getNetwork", "session": session})["network"]
        return [n["index"] for n in network["neurons"] if n["name"].startswith(prefix)]

    def connect_sensor(
        self,
        session: int,
        name: str,
        targets: list[list[int]],
        encoding: dict[str, Any],
        ingress_delay_ms: float | None = None,
        buffer_while_paused: bool = False,
    ) -> int:
        """Connects a sensor; `targets[c]` are the neurons channel c drives. Returns the sensor id."""
        spec: dict[str, Any] = {"name": name, "targets": targets, "encoding": encoding, "bufferWhilePaused": buffer_while_paused}
        if ingress_delay_ms is not None:
            spec["ingressDelayMs"] = ingress_delay_ms
        return self.request({"type": "connectSensor", "session": session, "sensor": spec})["sensor"]

    def send_samples(self, sensor: int, t0_ns: int, dt_ns: int, channels: int, values: Sequence[float]) -> None:
        """Sends samples (sample-major) taken `dt_ns` apart from the sensor's clock time `t0_ns`."""
        self._socket.send(encode_samples(sensor, self._seq, t0_ns, dt_ns, channels, values))
        self._seq += 1
        self._drain()

    def send_events(self, sensor: int, t0_ns: int, events: Iterable[tuple[int, int]]) -> None:
        """Sends events (channel, offset ns) for a sensor with spike encoding."""
        self._socket.send(encode_events(sensor, self._seq, t0_ns, events))
        self._seq += 1
        self._drain()

    def _drain(self) -> None:
        """Reads whatever has arrived (notices) without waiting, raising errors about earlier batches."""
        while True:
            try:
                message = self._socket.recv(timeout=0)
            except TimeoutError:
                return
            except ConnectionClosed:
                raise
            self._other(message)
