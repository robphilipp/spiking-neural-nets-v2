# Network description ("DNA", `.boo` files), sensors and projects

The full reference is `wiki/20171216/dna-defs.md`. The parser is
`application/spikes-utils/.../construction/parsing/DnaParser.scala` (Scala parser combinators, about 1,700 lines,
with tests in `DnaParserTest.scala`).

## Structure

```
(
GRP=[ (gid=group1), (gid=group2, hst=192.168.1.175, prt=2552) ],   // execution groups: local or remote host:port
NRN=[ (nid=input-11, grp=group1, nty=mi, mst=5 mV, inh=f, rfp=20 ms, rfb=0.1 µWb, mnp=0 mV, mpd=250 ms,
       mpr=2 ms, mpn=0.1 mV, wnm=6e-4, spp=1 mV, csp=1 m/s, ipb=0.1 mV, ipl=0.01 mV, ipd=1 h,
       WDF=(fnc=exp, dhl=100 s), SRP=(fcb=1, fcm=0.1, fct=100 ms, dpb=1, dpm=0.1, dpt=100 ms),
       WLF=(fnc=bnd, lwb=0.0, upb=5.0), LOC=(cst=ct, px1=-30 µm, px2=-30 µm, px3=0 µm)), ... ],
CON=[ (prn=input-1{1:7:1}, psn=hidden-1, cnw=1.5, eqw=1.5, lrn=stdp, cst=sig), ... ],   // ranges {start:end:step}, sets {1,3,5}
LRN=[ (fnc=stdp_hard, ina=0.035, inp=3 ms, exa=0.035, exp=3 ms), (fnc=flat) ],
CST=[ (fnc=sig, dhl=10 s), (fnc=none) ]
)
```

- Units are optional. The defaults are µm, ms, m/s, mV and Hz. Unit strings can contain Greek letters (`µ`).
- Bistable neurons add `lct` (firing threshold), `rst` (resting threshold) and `tfr` (tonic firing rate).
- `cnw` and `eqw` accept a random range `r0:1`.
- Coordinate systems: cartesian `ct`, cylindrical `cl`, spherical `sp`.
- **Groups map neurons to execution locations.** This is v1's version of "actor ID → physical location". Changing a
  group's host moves its neurons without changing the network's structure. v2's actor directory generalizes this.

## Sensor files (`*.sensor`, e-spikes-ui)

A sensor file is JavaScript that returns an RxJS `Observable<SensorOutput>`:
`{ sensorName, neuronIds: string[], time?: number, signal: { value, units: 'µV'|'mV' } }`. The UI compiles it in a
worker thread and streams the outputs to the server over the WebSocket. The server then sends
`Signal(now, world, value)` to the listed input neurons. Signals from the world are timestamped when they *arrive*.

## Project files (`*.spikes`)

These are JSON: `{ simulationName, timeFactor, simulationDuration, sensorFilePath, networkFilePath }`.

## For v2

- Keep the `.boo` format so existing networks and the Monaco language support carry over. Reimplement the parser in
  Rust (`nom`/`winnow`/`chumsky`, or `pest`). Use the 40+ `.boo` files under `application/old-code/spikes/test_data`
  plus `DnaParserTest.scala` as a conformance suite.
- Consider an alternate JSON/TOML/YAML serialization of the same model for programmatic generation.
- JavaScript sensors were convenient but tie input generation to the UI. In v2, sensors could be (a) TypeScript in
  the UI (as now), (b) Rust input actors (periodic, Poisson, file replay), or (c) external streams. Each needs an
  explicit time-stamping policy (see the time model in the actor plan).

## v2 parser (2026-11-22)

Implemented in `crates/spikes-dna` and lenient with older files (v1's defaults for missing keys; `fnc=stdp`,
`SNT`, `uwb` accepted). New keys: `nty=mr|br` with `RES=(frq, dmp, lcr, sep)`, `HIP=(tgt, str, win, lwb, upb)`,
per-connection `typ=exc|inh`, and `nam=` for learning and stickiness functions. The v1 test networks live in
`test-data/v1/` as a conformance suite. See `docs/plans/neurons/README.md` (N2).
