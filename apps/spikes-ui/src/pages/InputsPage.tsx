import { AudioLines, Download, Play, Square } from "lucide-react";
import type { Encoding, Neuron, ProtocolClient, Sensor } from "@spikes/client";
import { useMemo, useState } from "react";
import { NEVER } from "rxjs";
import {
  AxisLocation,
  Chart,
  ContinuousAxis,
  type Datum,
  defaultLineStyle,
  ScatterPlot,
  type SeriesLineStyle,
  seriesFrom,
  TimeSeriesDataSource,
} from "stream-charts";
import { useConnection } from "../connection";
import { startInput, stopInput, useActiveInput } from "../inputStreams";
import { compile, sample, type Signal, SignalError } from "../signal";
import {
  Choice,
  Field,
  Heading,
  Notice,
  NumberField,
  Panel,
  Row,
} from "@/components/kit";
import { Button } from "@/components/ui/button";
import { useChartColors } from "../theme";
import { useChartSettings, ZOOM, ZOOM_HELP } from "../chartSettings";
import { NeuronMatchField } from "../components/NeuronMatchField";
import { compileMatch, type NeuronMatch } from "../neuronMatch";
import { useSession } from "../store";

const PALETTE = [
  "#0072B2",
  "#E69F00",
  "#009E73",
  "#CC79A7",
  "#56B4E9",
  "#D55E00",
  "#F0E442",
  "#999999",
];
const DT_MS = 1; // sample spacing for designed signals
const PREVIEW_MS = 1000;

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

export function InputsPage() {
  const colors = useChartColors();
  const { zoomResets, resetZoom } = useChartSettings();
  const { client } = useConnection();
  const { id, network, state, replayEndMs } = useSession();
  const [match, setMatch] = useState<NeuronMatch>({ mode: "prefix", pattern: "input-" });
  const [kind, setKind] = useState<EncodingKind>("rate");
  const [a, setA] = useState(200);
  const [b, setB] = useState(6);
  const [formula, setFormula] = useState("floor(t / 100) % 7 == c % 7");
  const [seconds, setSeconds] = useState(5);
  const [message, setMessage] = useState<{ ok: boolean; text: string } | null>(
    null,
  );
  const activeInput = useActiveInput();
  const active = activeInput?.kind ?? null;

  const targets: Neuron[] = useMemo(
    () => {
      // (An invalid regular expression picks nothing: the field says why.)
      const compiled = compileMatch(match);
      return "error" in compiled ? [] : (network?.neurons ?? []).filter((n) => compiled.test(n.name));
    },
    [network, match],
  );
  const compiled = useMemo((): { signal: Signal } | { error: string } => {
    try {
      return { signal: compile(formula) };
    } catch (e) {
      return {
        error:
          e instanceof SignalError
            ? `${e.message} (at character ${e.position + 1})`
            : String(e),
      };
    }
  }, [formula]);
  const channels = targets.length;

  // A static preview of the first channels.
  const preview = useMemo(() => {
    if (!("signal" in compiled) || !channels) return null;
    const shown = Math.min(channels, PALETTE.length);
    const steps = PREVIEW_MS / 5;
    const values = sample(compiled.signal, channels, 5, 0, steps);
    const series = Array.from({ length: shown }, (_, c) =>
      seriesFrom<Datum>(
        targets[c]!.name,
        Array.from({ length: steps }, (_, s) => ({
          x: s * 5,
          y: values[s * channels + c]! + c * 1.2,
        })),
      ),
    );
    const styles = new Map<string, SeriesLineStyle>(
      series.map((s, i) => [
        s.name,
        { ...defaultLineStyle(), color: PALETTE[i]!, lineWidth: 1.5 },
      ]),
    );
    return {
      source: new TimeSeriesDataSource({
        initialData: series,
        generator: () => NEVER,
      }),
      styles,
      shown,
    };
  }, [compiled, channels, targets]);

  if (id === null || !network) {
    return (
      <Notice>
        Build a network on the Run page first; its neurons are the inputs'
        targets.
      </Notice>
    );
  }
  if (replayEndMs !== null) {
    return (
      <Notice>
        This session is a replay: it plays back what was recorded and takes no
        input.
      </Notice>
    );
  }
  const ready = state === "ready";
  const running = state === "running" || state === "paused";

  const connect = async (
    client: ProtocolClient,
    name: string,
    n: number,
    buffer: boolean,
  ) =>
    client.connectSensor(id, {
      name,
      targets: targets.slice(0, n).map((t) => [t.index]),
      encoding: encodingOf(kind, a, b),
      bufferWhilePaused: buffer,
      ingressDelayMs: buffer ? 1 : undefined,
    });

  const attempt = (f: () => Promise<void>) => async () => {
    setMessage(null);
    try {
      await f();
    } catch (e) {
      setMessage({
        ok: false,
        text: e instanceof Error ? e.message : String(e),
      });
    }
  };

  return (
    <div className="flex max-w-5xl flex-col gap-3">
      <Panel>
        <Heading>Signal</Heading>
        <Row>
          <div className="grid gap-1">
            <NeuronMatchField label="Target neurons" width={250} value={match} onChange={setMatch} testId="target-neurons" />
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
          <NumberField
            label={
              kind === "rate"
                ? "Hz at 1"
                : kind === "current"
                  ? "mV per unit"
                  : "Change per event"
            }
            width={130}
            value={a}
            onChange={setA}
          />
          {kind !== "current" && (
            <NumberField
              label="mV per event"
              width={120}
              value={b}
              onChange={setB}
            />
          )}
        </Row>
        <Field
          label="Value of channel c at time t (ms); n channels"
          value={formula}
          onChange={(e) => setFormula(e.target.value)}
          error={"error" in compiled}
          help={
            "error" in compiled
              ? compiled.error
              : "e.g. pulse(t, 200, 50) · sin(2*pi*t/500 + c) · floor(t/100) % 7 == c % 7 · t > 300 ? 1 : 0"
          }
          data-testid="formula"
          spellCheck={false}
          className="[&_input]:font-mono"
        />
        {preview && (
          <div data-testid="preview">
            <p className="text-xs text-muted-foreground">
              First {PREVIEW_MS / 1000} s of{" "}
              {preview.shown < channels
                ? `the first ${preview.shown} of ${channels}`
                : "all"}{" "}
              channels (offset for clarity). {ZOOM_HELP}{" "}
              <button
                type="button"
                className="underline"
                onClick={resetZoom}
                data-testid="reset-zoom"
              >
                Reset zoom
              </button>
            </p>
            <Chart
              key={zoomResets}
              chartId={10}
              width={900}
              height={200}
              dataSource={preview.source}
              color={colors.color}
              backgroundColor={colors.backgroundColor}
              seriesStyles={preview.styles}
            >
              <ContinuousAxis
                axisId="x-axis-1"
                location={AxisLocation.Bottom}
                domain={[0, PREVIEW_MS]}
                label="t (ms)"
              />
              <ContinuousAxis
                axisId="y-axis-1"
                location={AxisLocation.Left}
                domain={[-0.2, preview.shown * 1.2]}
                label="value"
              />
              <ScatterPlot {...ZOOM} />
            </Chart>
          </div>
        )}
      </Panel>

      <Panel>
        <Row>
          <NumberField
            label="Seconds"
            width={100}
            min={0.01}
            value={seconds}
            onChange={setSeconds}
          />
          <Button
            disabled={!client || !ready || !channels || !("signal" in compiled)}
            data-testid="inject"
            title="Inject the whole signal now, at exact times from the current time: every run with it is the same"
            onClick={attempt(async () => {
              if (!("signal" in compiled)) return;
              const sensor = await connect(
                client!,
                "designed signal",
                channels,
                true,
              );
              const total = Math.round((seconds * 1000) / DT_MS);
              const per = Math.max(
                1,
                Math.min(Math.floor(65_535), Math.floor(200_000 / channels)),
              );
              for (let first = 0; first < total; first += per) {
                const count = Math.min(per, total - first);
                sensor.sendSamples(
                  BigInt(Math.round(first * DT_MS * 1e6)),
                  DT_MS * 1e6,
                  channels,
                  sample(compiled.signal, channels, DT_MS, first, count),
                );
              }
              setMessage({
                ok: true,
                text: `Injected ${seconds} s of the signal into ${channels} neurons. Start the run on the Run page.`,
              });
            })}
          >
            <Download data-icon="inline-start" />
            Inject before running
          </Button>
          <Button
            variant="outline"
            disabled={
              !client ||
              !running ||
              active !== null ||
              !channels ||
              !("signal" in compiled)
            }
            data-testid="stream"
            title="Stream the signal as the run goes, like a sensor"
            onClick={attempt(async () => {
              if (!("signal" in compiled)) return;
              const signal = compiled.signal;
              const sensor = await connect(
                client!,
                "designed signal (live)",
                channels,
                false,
              );
              const stopStream = stream(
                sensor,
                channels,
                DT_MS,
                (first, count) => sample(signal, channels, DT_MS, first, count),
              );
              startInput({
                kind: "live",
                session: id,
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
              const sensor = await connect(
                client!,
                "microphone",
                channels,
                false,
              );
              const stopStream = stream(sensor, channels, 10, (_, count) => {
                analyser.getFloatFrequencyData(spectrum);
                const one = bands(spectrum, channels, audio.sampleRate);
                const values = new Float32Array(count * channels);
                for (let s = 0; s < count; s++) values.set(one, s * channels);
                return values;
              });
              startInput({
                kind: "microphone",
                session: id,
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
            <Button
              className="bg-warning text-black hover:bg-warning/80"
              onClick={stopInput}
              data-testid="stop-input"
            >
              <Square data-icon="inline-start" />
              Stop {active === "microphone" ? "the microphone" : "streaming"}
            </Button>
          )}
        </Row>
        <p className="text-xs text-muted-foreground">
          {ready
            ? "Inject before running: the signal is placed at exact times from now, so every run with it is the same."
            : running
              ? "Streaming follows the wall clock, like a real sensor."
              : "Build the network first."}
        </p>
        {message && (
          <Notice
            tone={message.ok ? "success" : "error"}
            onClose={() => setMessage(null)}
          >
            {message.text}
          </Notice>
        )}
      </Panel>
    </div>
  );
}
