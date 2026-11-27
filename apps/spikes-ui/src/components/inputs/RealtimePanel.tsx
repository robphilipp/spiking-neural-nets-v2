// Realtime input until phase I4 brings it into the input file: a designed signal streamed as the run goes, and the
// microphone, both through the sensor protocol into the running network. (Simulated signals are sources of the input
// file; they run in simulated time.)

import { AudioLines, Play, Square } from "lucide-react";
import type { Encoding, Neuron, ProtocolClient, Sensor } from "@spikes/client";
import { useMemo, useState } from "react";
import { Choice, Field, Heading, Notice, NumberField, Panel, Row } from "@/components/kit";
import { Button } from "@/components/ui/button";
import { useConnection } from "../../connection";
import { startInput, stopInput, useActiveInput } from "../../inputStreams";
import { compile, sample, type Signal, SignalError } from "../../signal";
import { matchNeurons, type NeuronMatch } from "../../neuronMatch";
import { useSession } from "../../store";
import { NeuronMatchField } from "../NeuronMatchField";

const DT_MS = 1; // sample spacing for designed signals

type EncodingKind = "current" | "rate" | "threshold";

function encodingOf(kind: EncodingKind, a: number, b: number): Encoding {
  switch (kind) {
    case "current":
      return { type: "current", gainMv: a };
    case "rate":
      return { type: "rate", maxHz: a, mv: b };
    case "threshold":
      return { type: "threshold", level: a, mv: b };
  }
}

/** Streams a source of samples into a session in 10 ms batches, on the sensor's own clock (wall time). Stops by
 *  itself if the connection goes. */
function stream(
  sensor: Sensor,
  channels: number,
  dtMs: number,
  next: (first: number, count: number) => ArrayLike<number>,
) {
  const start = performance.now();
  let sent = 0; // samples per channel
  const timer = setInterval(() => {
    const due = Math.floor((performance.now() - start) / dtMs);
    if (due <= sent) return;
    try {
      sensor.sendSamples(
        BigInt(Math.round(sent * dtMs * 1e6)),
        dtMs * 1e6,
        channels,
        next(sent, due - sent),
      );
    } catch {
      stopInput(); // not connected any more
      return;
    }
    sent = due;
  }, 10);
  return () => clearInterval(timer);
}

/** Log-spaced bands of an analyser's spectrum, scaled to 0…1. */
function bands(
  spectrum: Float32Array,
  channels: number,
  sampleRate: number,
): Float32Array {
  const out = new Float32Array(channels);
  const binHz = sampleRate / 2 / spectrum.length;
  const lo = Math.log(100);
  const hi = Math.log(Math.min(8000, sampleRate / 2));
  for (let c = 0; c < channels; c++) {
    const from = Math.max(
      0,
      Math.floor(Math.exp(lo + ((hi - lo) * c) / channels) / binHz),
    );
    const to = Math.max(
      from + 1,
      Math.floor(Math.exp(lo + ((hi - lo) * (c + 1)) / channels) / binHz),
    );
    let sum = 0;
    for (let b = from; b < to && b < spectrum.length; b++) sum += spectrum[b]!;
    const db = sum / (to - from);
    out[c] = Math.min(1, Math.max(0, (db + 90) / 60)); // −90 dB … −30 dB
  }
  return out;
}


/** Streams a designed signal or the microphone into the running network (realtime, until phase I4). */
export function RealtimePanel() {
  const { client } = useConnection();
  const { id, network, state, replayEndMs } = useSession();
  const [match, setMatch] = useState<NeuronMatch>({ mode: "prefix", pattern: "input-" });
  const [kind, setKind] = useState<EncodingKind>("rate");
  const [a, setA] = useState(200);
  const [b, setB] = useState(6);
  const [formula, setFormula] = useState("floor(t / 100) % 7 == c % 7");
  const [message, setMessage] = useState<{ ok: boolean; text: string } | null>(null);
  const activeInput = useActiveInput();
  const active = activeInput?.kind ?? null;
  const targets: Neuron[] = useMemo(() => (network ? matchNeurons(network.neurons, match) : []), [network, match]);
  const compiled = useMemo((): { signal: Signal } | { error: string } => {
    try {
      return { signal: compile(formula) };
    } catch (e) {
      return { error: e instanceof SignalError ? `${e.message} (at character ${e.position + 1})` : String(e) };
    }
  }, [formula]);
  const channels = targets.length;
  const running = state === "running" || state === "paused";
  const available = id !== null && network !== null && replayEndMs === null;

  const connect = async (client: ProtocolClient, name: string, n: number) =>
    client.connectSensor(id!, { name, targets: targets.slice(0, n).map((t) => [t.index]), encoding: encodingOf(kind, a, b), bufferWhilePaused: false });

  const attempt = (f: () => Promise<void>) => async () => {
    setMessage(null);
    try {
      await f();
    } catch (e) {
      setMessage({ ok: false, text: e instanceof Error ? e.message : String(e) });
    }
  };

  return (
    <Panel data-testid="realtime-panel">
      <Heading>Realtime (until phase I4)</Heading>
      <p className="text-xs text-muted-foreground">
        Streams into a running network as it goes, by the wall clock, like a sensor: not reproducible, and not part of the input file. (Realtime
        sources will move into the file in phase I4.) Start the run on the Run page first.
      </p>
      {!available ? (
        <p className="text-sm text-muted-foreground">Build a network on the Run page to stream into it.</p>
      ) : (
        <>
          <Row>
            <div className="grid gap-1">
              <NeuronMatchField label="Stream into neurons" width={250} value={match} onChange={setMatch} neurons={network.neurons} testId="live-target-neurons" />
              <p className="text-xs text-muted-foreground">{`${channels} channel${channels === 1 ? "" : "s"}, one per neuron`}</p>
            </div>
            <Choice
              label="Encoding"
              width={160}
              value={kind}
              onChange={setKind}
              options={[
                { value: "rate", label: "rate (Poisson)" },
                { value: "current", label: "current" },
                { value: "threshold", label: "threshold (delta)" },
              ]}
            />
            <NumberField label={kind === "rate" ? "Hz at 1" : kind === "current" ? "mV per unit" : "Change per event"} width={130} value={a} onChange={setA} />
            {kind !== "current" && <NumberField label="mV per event" width={120} value={b} onChange={setB} />}
          </Row>
          <Field
            label="Live signal: value of channel c at time t (ms); n channels"
            value={formula}
            onChange={(e) => setFormula(e.target.value)}
            error={"error" in compiled}
            help={"error" in compiled ? compiled.error : "e.g. pulse(t, 200, 50) · sin(2*pi*t/500 + c) · floor(t/100) % 7 == c % 7"}
            data-testid="live-formula"
            spellCheck={false}
            className="[&_input]:font-mono"
          />
          <Row>
            <Button
              variant="outline"
              disabled={!client || !running || active !== null || !channels || !("signal" in compiled)}
              data-testid="stream"
              title="Stream the signal as the run goes, like a sensor"
              onClick={attempt(async () => {
                if (!("signal" in compiled)) return;
                const signal = compiled.signal;
                const sensor = await connect(client!, "designed signal (live)", channels);
                const stopStream = stream(sensor, channels, DT_MS, (first, count) => sample(signal, channels, DT_MS, first, count));
                startInput({
                  kind: "live",
                  session: id!,
                  stop: () => {
                    stopStream();
                    void sensor.disconnect().catch(() => {}); // the engine forgets it
                  },
                });
              })}
            >
              <Play data-icon="inline-start" />
              Stream live
            </Button>
          <Button
            variant="outline"
            disabled={!client || !running || active !== null || !channels}
            data-testid="microphone"
            title="The microphone's spectrum in log-spaced bands, one per target neuron"
            onClick={attempt(async () => {
              // The raw sound: echo cancellation, noise suppression and gain control would reshape it (noise
              // suppression removes steady tones altogether).
              const media = await navigator.mediaDevices.getUserMedia({
                audio: {
                  echoCancellation: false,
                  noiseSuppression: false,
                  autoGainControl: false,
                },
              });
              const audio = new AudioContext();
              const analyser = audio.createAnalyser();
              analyser.fftSize = 2048;
              audio.createMediaStreamSource(media).connect(analyser);
              const spectrum = new Float32Array(analyser.frequencyBinCount);
              const sensor = await connect(client!, "microphone", channels);
              const stopStream = stream(sensor, channels, 10, (_, count) => {
                analyser.getFloatFrequencyData(spectrum);
                const one = bands(spectrum, channels, audio.sampleRate);
                const values = new Float32Array(count * channels);
                for (let s = 0; s < count; s++) values.set(one, s * channels);
                return values;
              });
              startInput({
                kind: "microphone",
                session: id!,
                stop: () => {
                  stopStream();
                  void sensor.disconnect().catch(() => {});
                  media.getTracks().forEach((t) => t.stop());
                  void audio.close();
                },
              });
            })}
          >
            <AudioLines data-icon="inline-start" />
            Microphone
          </Button>
            {active && (
              <Button className="bg-warning text-black hover:bg-warning/80" onClick={stopInput} data-testid="stop-input">
                <Square data-icon="inline-start" />
                Stop {active === "microphone" ? "the microphone" : "streaming"}
              </Button>
            )}
          </Row>
          {message && (
            <Notice tone={message.ok ? "success" : "error"} onClose={() => setMessage(null)}>
              {message.text}
            </Notice>
          )}
        </>
      )}
    </Panel>
  );
}
