"""Tests against the Rust golden files and a real spikes-server (build it first: cargo build -p spikes-server)."""

import os
import re
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / "clients" / "python"))

from spikes_sensor import SensorClient, SpikesError, encode_events, encode_samples  # noqa: E402

GOLDEN = ROOT / "packages" / "spikes-client" / "test" / "golden"


class Encoding(unittest.TestCase):
    def test_matches_the_rust_encoder(self):
        self.assertEqual(encode_samples(3, 7, 1 << 40, 1_000_000, 2, [0.5, -1.0, 2.0, 3.5]), (GOLDEN / "samples.bin").read_bytes())
        self.assertEqual(encode_events(1, 0, 5, [(0, 10), (3, 20), (1, 30)]), (GOLDEN / "events.bin").read_bytes())

    def test_rejects_partial_samples(self):
        with self.assertRaises(ValueError):
            encode_samples(0, 0, 0, 1, 3, [1.0, 2.0])


class Server(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        binary = ROOT / "target" / "debug" / "spikes-server"
        cls.recordings = tempfile.mkdtemp()
        cls.process = subprocess.Popen([str(binary), "--listen", "127.0.0.1:0", "--recordings", cls.recordings],
                                       stdout=subprocess.PIPE, text=True)
        line = cls.process.stdout.readline()
        cls.url = re.search(r"http://[\d.:]+", line).group(0)

    @classmethod
    def tearDownClass(cls):
        cls.process.kill()
        cls.process.wait()
        cls.process.stdout.close()

    def test_a_sensor_drives_neurons(self):
        with SensorClient(self.url) as c:
            neurons = ", ".join(f"(nid=in-{i}, mst=1 mV, mpn=0 mV, LOC=(cst=ct, px1={i}, px2=0, px3=0))" for i in range(4))
            s = c.create_session("py", f"(NRN=[{neurons}])")
            c.build(s)
            targets = c.neurons_named(s, "in-")
            sensor = c.connect_sensor(s, "probe", [[n] for n in targets], {"type": "current", "gainMv": 5.0})
            c.start(s, speed=1.0)
            c.send_samples(sensor, 0, 1_000_000, 4, [1.0, 0.0, 1.0, 0.0])
            import time
            time.sleep(0.1)
            self.assertEqual([c.query_neuron(s, n)["spikes"] for n in targets], [1, 0, 1, 0])
            with self.assertRaises(SpikesError) as e:
                c.request({"type": "pause", "session": 99})
            self.assertEqual(e.exception.code, "notFound")
            c.stop(s)

    def test_the_microphone_example_runs_on_a_synthetic_chirp(self):
        out = subprocess.run([sys.executable, str(ROOT / "clients" / "python" / "examples" / "microphone.py"),
                              "--server", self.url, "--synthetic", "--seconds", "2"],
                             capture_output=True, text=True, timeout=60,
                             env={**os.environ, "PYTHONPATH": str(ROOT / "clients" / "python")})
        self.assertEqual(out.returncode, 0, out.stderr)
        spikes = [int(n) for n in re.search(r"spikes per band: \[(.*)\]", out.stdout).group(1).split(",")]
        self.assertEqual(len(spikes), 64)
        self.assertGreater(sum(1 for n in spikes if n > 0), 32, f"the chirp sweeps most bands: {spikes}")


if __name__ == "__main__":
    unittest.main()
