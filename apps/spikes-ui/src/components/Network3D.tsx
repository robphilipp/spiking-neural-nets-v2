// The network in 3D: neurons at their positions, spikes flashing, connections of the selection; click to inspect a
// neuron, shift-drag a box to select a region (it becomes the raster's rows). Small networks are instanced spheres;
// big ones are point sprites (one vertex per neuron), so 100k+ neurons are one cheap draw call either way.

import { cssColor, useThemeMode } from "../theme";
import type { Frame, Network, SensorInfo } from "@spikes/client";
import { useEffect, useRef, useState } from "react";
import type { Observable } from "rxjs";
import * as THREE from "three";
import { OrbitControls } from "three/examples/jsm/controls/OrbitControls.js";

const FLASH_MS = 200; // wall-clock fade of a spike flash
/** Above this many neurons, draw them as point sprites (one vertex each) instead of spheres. */
const SPHERES_UP_TO = 5000;
/** Up to this many connections, all are drawn faintly. */
const MAX_FAINT_LINES = 50_000;

/** The neurons as something to draw, colour and pick: instanced spheres, or points for big networks. */
interface Drawable {
  object: THREE.Object3D;
  /** A neuron's colour, and how lit it is (0…1: a lit sphere swells). */
  setColor(i: number, c: THREE.Color, lit: number): void;
  /** After colour changes. */
  commit(): void;
  /** The neuron a raycast hit, if any. */
  hit(ray: THREE.Raycaster): number | undefined;
  dispose(): void;
}

function spheres(network: Network, radius: number, pickRadius: number, base: THREE.Color[]): Drawable {
  const n = network.neurons.length;
  const geometry = new THREE.SphereGeometry(radius, 12, 8);
  const material = new THREE.MeshLambertMaterial();
  const mesh = new THREE.InstancedMesh(geometry, material, Math.max(n, 1));
  const matrix = new THREE.Matrix4();
  network.neurons.forEach((neuron, i) => {
    matrix.makeTranslation(...neuron.position);
    mesh.setMatrixAt(i, matrix);
    mesh.setColorAt(i, base[i]!);
  });
  mesh.count = n;
  return {
    object: mesh,
    setColor: (i, c, lit) => {
      mesh.setColorAt(i, c);
      const [x, y, z] = network.neurons[i]!.position;
      const k = 1 + 0.6 * lit;
      matrix.makeScale(k, k, k).setPosition(x, y, z);
      mesh.setMatrixAt(i, matrix);
    },
    commit: () => {
      if (mesh.instanceColor) mesh.instanceColor.needsUpdate = true;
      mesh.instanceMatrix.needsUpdate = true;
    },
    // The neuron nearest the viewer whose centre passes within the pick radius (the spheres are small to click).
    hit: (ray) => {
      const limit = pickRadius * pickRadius;
      const p = new THREE.Vector3();
      let best: number | undefined;
      let bestDistance = Infinity;
      network.neurons.forEach((neuron, i) => {
        p.set(...neuron.position);
        if (ray.ray.distanceSqToPoint(p) > limit) return;
        const along = p.sub(ray.ray.origin).dot(ray.ray.direction);
        if (along > 0 && along < bestDistance) {
          bestDistance = along;
          best = i;
        }
      });
      return best;
    },
    dispose: () => {
      geometry.dispose();
      material.dispose();
    },
  };
}

function points(network: Network, radius: number, pickRadius: number, base: THREE.Color[]): Drawable {
  const n = network.neurons.length;
  const positions = new Float32Array(n * 3);
  const colours = new Float32Array(n * 3);
  network.neurons.forEach((neuron, i) => {
    positions.set(neuron.position, i * 3);
    colours.set([base[i]!.r, base[i]!.g, base[i]!.b], i * 3);
  });
  const geometry = new THREE.BufferGeometry();
  geometry.setAttribute("position", new THREE.BufferAttribute(positions, 3));
  const colour = new THREE.BufferAttribute(colours, 3);
  geometry.setAttribute("color", colour);
  // A round sprite: a small disc drawn once.
  const disc = document.createElement("canvas");
  disc.width = disc.height = 32;
  const g = disc.getContext("2d")!;
  g.beginPath();
  g.arc(16, 16, 15, 0, Math.PI * 2);
  g.fillStyle = "#fff";
  g.fill();
  const texture = new THREE.CanvasTexture(disc);
  const material = new THREE.PointsMaterial({ size: radius * 2, vertexColors: true, map: texture, alphaTest: 0.5, sizeAttenuation: true });
  const cloud = new THREE.Points(geometry, material);
  return {
    object: cloud,
    setColor: (i, c) => {
      colours[i * 3] = c.r;
      colours[i * 3 + 1] = c.g;
      colours[i * 3 + 2] = c.b;
    },
    commit: () => {
      colour.needsUpdate = true;
    },
    hit: (ray) => {
      ray.params.Points = { threshold: pickRadius };
      return ray.intersectObject(cloud)[0]?.index;
    },
    dispose: () => {
      geometry.dispose();
      material.dispose();
      texture.dispose();
    },
  };
}

/** The median distance from a neuron to its nearest neighbour (over a sample of up to 300 neurons). */
function nearestSpacing(network: Network): number {
  const ns = network.neurons;
  if (ns.length < 2) return Infinity;
  const step = Math.max(1, Math.floor(ns.length / 300));
  const nearest: number[] = [];
  for (let i = 0; i < ns.length; i += step) {
    const [x, y, z] = ns[i]!.position;
    let best = Infinity;
    for (let j = 0; j < ns.length; j++) {
      if (j === i) continue;
      const [a, b, c] = ns[j]!.position;
      const d = (a - x) ** 2 + (b - y) ** 2 + (c - z) ** 2;
      if (d > 0 && d < best) best = d;
    }
    if (best < Infinity) nearest.push(Math.sqrt(best));
  }
  nearest.sort((a, b) => a - b);
  return nearest.length ? nearest[Math.floor(nearest.length / 2)]! : Infinity;
}

// Muted Okabe–Ito hues (blue, reddish purple) at rest, so a spike's bright orange (a hue neither has) stands out;
// still distinguishable for colour-blind viewers.
const EXCITATORY = 0x8fb3cf;
const INHIBITORY = 0xd3a6c3;
const FLASH = 0xff8c00;
/** Sensors: Okabe–Ito bluish green, apart from both neuron kinds and the flash. */
const SENSOR = 0x009e73;
/** Lines from one sensor to its neurons, at most. */
const MAX_SENSOR_LINES = 5000;

/** Frees a group's geometries, materials and textures. */
function disposeGroup(group: THREE.Group) {
  group.traverse((o) => {
    if (o instanceof THREE.Mesh || o instanceof THREE.LineSegments || o instanceof THREE.Sprite) {
      if ("geometry" in o && o.geometry) o.geometry.dispose();
      const material = o.material as THREE.Material & { map?: THREE.Texture | null };
      material.map?.dispose();
      material.dispose();
    }
  });
}

/** A text label that always faces the camera (scale it to the scene). */
function textSprite(text: string, color: string): THREE.Sprite {
  const canvas = document.createElement("canvas");
  canvas.width = 512;
  canvas.height = 96;
  const g = canvas.getContext("2d")!;
  g.font = "500 52px system-ui, sans-serif";
  g.fillStyle = color;
  g.textAlign = "center";
  g.textBaseline = "middle";
  g.fillText(text.length > 24 ? `${text.slice(0, 23)}…` : text, 256, 48);
  const sprite = new THREE.Sprite(new THREE.SpriteMaterial({ map: new THREE.CanvasTexture(canvas), transparent: true, depthTest: false }));
  sprite.scale.set(1, 96 / 512, 1);
  return sprite;
}

interface Props {
  network: Network;
  frames: Observable<Frame>;
  width: number;
  height: number;
  /** Neurons whose connections are drawn (and which are highlighted). */
  selected: number[];
  /** Connected sensors: drawn above the network, with lines to the neurons they drive. */
  sensors?: SensorInfo[];
  onPick: (neuron: number) => void;
  onRegion: (neurons: number[]) => void;
}

export function Network3D({ network, frames, width, height, selected, sensors = [], onPick, onRegion }: Props) {
  const mode = useThemeMode();
  const mount = useRef<HTMLDivElement>(null);
  const scene = useRef<{
    renderer: THREE.WebGLRenderer;
    camera: THREE.PerspectiveCamera;
    neurons: Drawable;
    /** The selected neurons' connections (every connection is drawn faintly besides). */
    lines: THREE.LineSegments;
    flash: Float32Array;
    /** Neurons currently lit (so each frame touches only them). */
    lit: Set<number>;
    base: THREE.Color[];
    controls: OrbitControls;
    /** Something changed: draw the next animation frame. */
    invalidate: () => void;
    bounds: THREE.Box3;
    size: number;
    radius: number;
    /** The sensors' markers, labels and lines. */
    sensors: THREE.Group;
  } | null>(null);
  /** Counts scene builds (a new network or theme), so the selection's lines are redrawn into each. */
  const [built, setBuilt] = useState(0);
  const [box, setBox] = useState<{ x0: number; y0: number; x1: number; y1: number } | null>(null);
  const callbacks = useRef({ onPick, onRegion });
  callbacks.current = { onPick, onRegion };

  // Build the scene for this network.
  useEffect(() => {
    const el = mount.current;
    if (!el) return;
    const n = network.neurons.length;
    const renderer = new THREE.WebGLRenderer({ antialias: true, preserveDrawingBuffer: true });
    renderer.setPixelRatio(window.devicePixelRatio);
    renderer.setSize(width, height);
    renderer.setClearColor(cssColor("--card"));
    el.appendChild(renderer.domElement);

    // Fit the network into view.
    const bounds = new THREE.Box3();
    for (const neuron of network.neurons) bounds.expandByPoint(new THREE.Vector3(...neuron.position));
    const centre = bounds.getCenter(new THREE.Vector3());
    const size = Math.max(bounds.getSize(new THREE.Vector3()).length(), 1);
    const camera = new THREE.PerspectiveCamera(45, width / height, size / 1000, size * 20);
    camera.position.copy(centre).add(new THREE.Vector3(0, -size * 0.2, size * 1.4));
    const controls = new OrbitControls(camera, renderer.domElement);
    controls.target.copy(centre);
    controls.update();

    const world = new THREE.Scene();
    world.add(new THREE.AmbientLight(0xffffff, 1.2));
    const light = new THREE.DirectionalLight(0xffffff, 1.5);
    light.position.set(1, 1, 2);
    world.add(light);

    // Neuron size: a small fraction of the typical spacing, so the structure and connections show; clicks reach a
    // little further than the drawn neuron.
    const spacing = Math.min(nearestSpacing(network), size / Math.cbrt(Math.max(n, 1)));
    const radius = Math.max(spacing * 0.22, size / 4000);
    const pickRadius = Math.max(spacing * 0.4, radius);
    const excitatory = new THREE.Color(EXCITATORY);
    const inhibitory = new THREE.Color(INHIBITORY);
    const base = network.neurons.map((neuron) => (neuron.inhibitory ? inhibitory : excitatory));
    const neurons = n <= SPHERES_UP_TO ? spheres(network, radius, pickRadius, base) : points(network, radius, pickRadius, base);
    world.add(neurons.object);

    // Every connection, faintly (up to a limit: beyond it they'd be a fog), and the selection's, strongly.
    const all: number[] = [];
    if (network.synapses.length <= MAX_FAINT_LINES) {
      for (const syn of network.synapses) all.push(...network.neurons[syn.pre]!.position, ...network.neurons[syn.post]!.position);
    }
    const allGeometry = new THREE.BufferGeometry().setAttribute("position", new THREE.Float32BufferAttribute(all, 3));
    // The theme's text colours carry their own alpha, which three.js ignores: use plain ink and set the opacity.
    const ink = mode === "dark" ? 0xffffff : 0x000000;
    const faint = new THREE.LineSegments(
      allGeometry,
      new THREE.LineBasicMaterial({ color: ink, transparent: true, opacity: n > 500 ? 0.07 : 0.18 }),
    );
    world.add(faint);
    const lines = new THREE.LineSegments(
      new THREE.BufferGeometry(),
      new THREE.LineBasicMaterial({ color: ink, transparent: true, opacity: 0.7 }),
    );
    world.add(lines);

    const flash = new Float32Array(n); // wall time (ms) each neuron last spiked
    const litSet = new Set<number>();
    const flashColour = new THREE.Color(FLASH);
    const colour = new THREE.Color();
    let frame = 0;
    // Draw only when something changed (a flash, the camera, a resize, the connection lines): a paused or quiet
    // network costs nothing.
    let dirty = true;
    const invalidate = () => {
      dirty = true;
    };
    controls.addEventListener("change", invalidate);
    const draw = () => {
      frame = requestAnimationFrame(draw);
      controls.update();
      if (!dirty && !litSet.size) return;
      dirty = false;
      if (litSet.size) {
        const now = performance.now();
        for (const i of litSet) {
          const age = now - flash[i]!;
          const lit = Math.max(0, 1 - age / FLASH_MS);
          colour.copy(base[i]!).lerp(flashColour, lit);
          neurons.setColor(i, colour, lit);
          if (age >= FLASH_MS) litSet.delete(i); // back to its base colour: no more work for it
        }
        neurons.commit();
      }
      renderer.render(world, camera);
    };
    draw();
    const sensorGroup = new THREE.Group();
    world.add(sensorGroup);
    scene.current = { renderer, camera, neurons, lines, flash, lit: litSet, base, controls, invalidate, bounds, size, radius, sensors: sensorGroup };
    setBuilt((b) => b + 1); // the selection's lines go into the new scene
    return () => {
      cancelAnimationFrame(frame);
      controls.dispose();
      neurons.dispose();
      lines.geometry.dispose();
      allGeometry.dispose();
      disposeGroup(sensorGroup);
      renderer.dispose();
      renderer.domElement.remove();
      scene.current = null;
    };
    // The scene is rebuilt for a new network or theme; size changes are handled below.
  }, [network, mode]); // eslint-disable-line react-hooks/exhaustive-deps

  useEffect(() => {
    const s = scene.current;
    if (!s) return;
    s.renderer.setSize(width, height);
    s.camera.aspect = width / height;
    s.camera.updateProjectionMatrix();
    s.invalidate();
  }, [width, height]);

  // Spikes flash their neurons.
  useEffect(() => {
    const byCell = new Map<number, number[]>();
    network.neurons.forEach((n) => {
      let list = byCell.get(n.cell);
      if (!list) byCell.set(n.cell, (list = []));
      list.push(n.index);
    });
    const sub = frames.subscribe((f) => {
      const s = scene.current;
      if (!s) return;
      const now = performance.now();
      const light = (n: number) => {
        s.flash[n] = now;
        s.lit.add(n);
      };
      for (const n of f.spikes?.neuron ?? []) light(n);
      for (const n of f.spikeCounts?.neuron ?? []) light(n);
      // Cells (the lowest level of detail) say how many spikes, not which neurons: light that many of the cell's
      // neurons, at random, so the display shows the real amount of activity.
      const bins = f.spikeBins;
      if (bins) {
        bins.cell.forEach((cell, i) => {
          const members = byCell.get(cell);
          if (!members?.length) return;
          for (let k = Math.min(bins.count[i]!, members.length); k > 0; k--) light(members[(Math.random() * members.length) | 0]!);
        });
      }
    });
    return () => sub.unsubscribe();
  }, [frames, network]);

  // Connections of the selected neurons (both directions).
  useEffect(() => {
    const s = scene.current;
    if (!s) return;
    const picks = new Set(selected);
    const points: number[] = [];
    for (const syn of network.synapses) {
      if (!picks.has(syn.pre) && !picks.has(syn.post)) continue;
      points.push(...network.neurons[syn.pre]!.position, ...network.neurons[syn.post]!.position);
      if (points.length > 6 * 20_000) break;
    }
    s.lines.geometry.dispose();
    s.lines.geometry = new THREE.BufferGeometry().setAttribute("position", new THREE.Float32BufferAttribute(points, 3));
    s.invalidate();
  }, [selected, network, built]);

  // Sensors: a diamond each, above the network near the neurons it drives, labelled, with lines to them. Rebuilt when
  // the sensors change; a sensor glows while its data flows.
  const shape = sensors.map((x) => `${x.id}:${x.name}:${x.channels.map((c) => c.join(",")).join(";")}`).join("|");
  const samples = useRef(new Map<number, number>());
  useEffect(() => {
    const s = scene.current;
    if (!s) return;
    disposeGroup(s.sensors);
    s.sensors.clear();
    const top = s.bounds.max.y + s.size * 0.18;
    sensors.forEach((sensor, k) => {
      const targets = [...new Set(sensor.channels.flat())].filter((i) => network.neurons[i]);
      const centre = new THREE.Vector3();
      for (const i of targets) centre.add(new THREE.Vector3(...network.neurons[i]!.position));
      if (targets.length) centre.divideScalar(targets.length);
      else s.bounds.getCenter(centre);
      // Spread several sensors out sideways so they don't overlap.
      const at = new THREE.Vector3(centre.x + (k - (sensors.length - 1) / 2) * s.size * 0.12, top, centre.z);
      const marker = new THREE.Mesh(
        new THREE.OctahedronGeometry(Math.max(s.radius * 3, s.size * 0.015)),
        new THREE.MeshLambertMaterial({ color: SENSOR, emissive: SENSOR, emissiveIntensity: 0.2 }),
      );
      marker.position.copy(at);
      marker.userData = { sensor: sensor.id };
      s.sensors.add(marker);
      const label = textSprite(sensor.name, mode === "dark" ? "#e5e5e5" : "#262626");
      label.position.copy(at).add(new THREE.Vector3(0, s.size * 0.07, 0));
      label.scale.multiplyScalar(s.size * 0.3);
      s.sensors.add(label);
      const points: number[] = [];
      for (const i of targets.slice(0, MAX_SENSOR_LINES)) points.push(at.x, at.y, at.z, ...network.neurons[i]!.position);
      const lines = new THREE.LineSegments(
        new THREE.BufferGeometry().setAttribute("position", new THREE.Float32BufferAttribute(points, 3)),
        new THREE.LineBasicMaterial({ color: SENSOR, transparent: true, opacity: 0.45 }),
      );
      s.sensors.add(lines);
    });
    s.invalidate();
    // (Only when the sensors' shape changes; their counters are handled below.)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [shape, network, built, mode]);
  useEffect(() => {
    const s = scene.current;
    if (!s) return;
    for (const sensor of sensors) {
      const before = samples.current.get(sensor.id);
      samples.current.set(sensor.id, sensor.samples);
      const active = before !== undefined && sensor.samples > before;
      for (const child of s.sensors.children) {
        if (child instanceof THREE.Mesh && child.userData.sensor === sensor.id) {
          (child.material as THREE.MeshLambertMaterial).emissiveIntensity = active ? 1 : 0.2;
          child.scale.setScalar(active ? 1.35 : 1);
        }
      }
    }
    s.invalidate();
  }, [sensors]);

  // Click: pick the neuron under the pointer. Shift-drag: select the neurons inside the box.
  const pointer = (e: React.PointerEvent) => {
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
    return { x: e.clientX - rect.left, y: e.clientY - rect.top };
  };
  const onPointerDown = (e: React.PointerEvent) => {
    const p = pointer(e);
    if (e.shiftKey && scene.current) {
      scene.current.controls.enabled = false;
      setBox({ x0: p.x, y0: p.y, x1: p.x, y1: p.y });
    }
    (e.currentTarget as HTMLElement).dataset.down = `${p.x},${p.y}`;
  };
  const onPointerMove = (e: React.PointerEvent) => {
    if (box) {
      const p = pointer(e);
      setBox({ ...box, x1: p.x, y1: p.y });
    }
  };
  const onPointerUp = (e: React.PointerEvent) => {
    const s = scene.current;
    if (!s) return;
    const p = pointer(e);
    const [dx, dy] = ((e.currentTarget as HTMLElement).dataset.down ?? "0,0").split(",").map(Number) as [number, number];
    if (box) {
      const [x0, x1] = [Math.min(box.x0, p.x), Math.max(box.x0, p.x)];
      const [y0, y1] = [Math.min(box.y0, p.y), Math.max(box.y0, p.y)];
      const v = new THREE.Vector3();
      const inside = network.neurons
        .filter((n) => {
          v.set(...n.position).project(s.camera);
          const sx = ((v.x + 1) / 2) * width;
          const sy = ((1 - v.y) / 2) * height;
          return v.z < 1 && sx >= x0 && sx <= x1 && sy >= y0 && sy <= y1;
        })
        .map((n) => n.index);
      setBox(null);
      s.controls.enabled = true;
      if (inside.length) callbacks.current.onRegion(inside);
      return;
    }
    if (Math.hypot(p.x - dx, p.y - dy) > 4) return; // a drag (orbit), not a click
    const ray = new THREE.Raycaster();
    ray.setFromCamera(new THREE.Vector2((p.x / width) * 2 - 1, -(p.y / height) * 2 + 1), s.camera);
    const hit = s.neurons.hit(ray);
    if (hit !== undefined) callbacks.current.onPick(hit);
  };

  return (
    <div className="flex flex-col gap-1">
      <div
        ref={mount}
        data-testid="network-3d"
        className="relative cursor-grab touch-none"
        style={{ width, height }}
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
      >
        {box && (
          <div
            className="pointer-events-none absolute border border-highlight bg-foreground/5"
            style={{
              left: Math.min(box.x0, box.x1),
              top: Math.min(box.y0, box.y1),
              width: Math.abs(box.x1 - box.x0),
              height: Math.abs(box.y1 - box.y0),
              borderColor: "var(--highlight)",
            }}
          />
        )}
      </div>
      <p className="text-xs text-muted-foreground">
        Drag to orbit, scroll to zoom, click a neuron to inspect it, shift-drag to select a region for the raster.
        {sensors.length > 0 && " Green diamonds are sensors, with lines to the neurons they drive; they light up while their data flows."}
      </p>
    </div>
  );
}
