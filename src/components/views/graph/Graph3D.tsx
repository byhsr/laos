import { useEffect, useMemo } from 'react';
import { Canvas } from '@react-three/fiber';
import { Line, OrbitControls } from '@react-three/drei';
import * as THREE from 'three';
import { NODE_COLOR, type GEdge, type GNode } from './shared';
import type { Vec } from './layout';

const ACCENT = '#d7ff38';
const LABEL_W = 256;
const LABEL_H = 48;

// Labels are drawn into a canvas rather than loaded as a webfont: the desktop
// webview has no network, and troika-style text would need a font file.
function labelTexture(text: string, color: string): THREE.CanvasTexture {
  const c = document.createElement('canvas');
  c.width = LABEL_W;
  c.height = LABEL_H;
  const ctx = c.getContext('2d');
  if (ctx) {
    const shown = text.length > 20 ? `${text.slice(0, 19)}…` : text;
    ctx.font = '500 26px "DM Mono", monospace';
    ctx.textAlign = 'center';
    ctx.textBaseline = 'middle';
    ctx.fillStyle = color;
    ctx.fillText(shown, LABEL_W / 2, LABEL_H / 2);
  }
  const tex = new THREE.CanvasTexture(c);
  tex.anisotropy = 4;
  return tex;
}

function Node({ node, position, radius, color, labelColor, dim, selected, onHover, onSelect }: {
  node: GNode; position: Vec; radius: number; color: string; labelColor: string;
  dim: boolean; selected: boolean; onHover: (id: string | null) => void; onSelect: (id: string) => void;
}) {
  const texture = useMemo(() => labelTexture(node.label, labelColor), [node.label, labelColor]);
  useEffect(() => () => texture.dispose(), [texture]);
  // Memory globs read as a softer mass than a hard capability node.
  const opacity = dim ? 0.18 : node.kind === 'memory' ? 0.78 : 1;

  return (
    <group position={position}>
      <mesh
        onClick={(e) => { e.stopPropagation(); onSelect(node.id); }}
        onPointerOver={(e) => { e.stopPropagation(); onHover(node.id); }}
        onPointerOut={() => onHover(null)}
      >
        <icosahedronGeometry args={[radius, 1]} />
        <meshStandardMaterial
          color={color}
          emissive={selected ? color : '#000000'}
          emissiveIntensity={selected ? 0.55 : 0}
          roughness={0.45}
          metalness={0.1}
          transparent
          opacity={opacity}
        />
      </mesh>
      <sprite position={[0, radius + 0.6, 0]} scale={[3.4, 0.64, 1]}>
        <spriteMaterial map={texture} transparent opacity={dim ? 0.25 : 0.95} depthWrite={false} />
      </sprite>
    </group>
  );
}

export default function Graph3D({ nodes, edges, positions, focus, connected, selected, spin, onHover, onSelect }: {
  nodes: GNode[]; edges: GEdge[]; positions: Map<string, Vec>;
  focus: string | null; connected: Set<string>; selected: string | null; spin: boolean;
  onHover: (id: string | null) => void; onSelect: (id: string) => void;
}) {
  // The scene is WebGL, so the theme has to be read as real colours rather than
  // CSS variables. Taken once when the view opens.
  const { backdrop, labelColor } = useMemo(() => {
    const cs = getComputedStyle(document.documentElement);
    return {
      backdrop: cs.getPropertyValue('--color-background').trim() || '#191919',
      labelColor: cs.getPropertyValue('--color-muted').trim() || '#8a8a8a',
    };
  }, []);

  const degree = useMemo(() => {
    const m = new Map<string, number>();
    edges.forEach((e) => { m.set(e.from, (m.get(e.from) ?? 0) + 1); m.set(e.to, (m.get(e.to) ?? 0) + 1); });
    return m;
  }, [edges]);

  const distance = useMemo(() => {
    let max = 1;
    positions.forEach((p) => { max = Math.max(max, Math.sqrt(p[0] * p[0] + p[1] * p[1] + p[2] * p[2])); });
    return max * 2.3 + 8;
  }, [positions]);

  const dimmed = (id: string) => !!focus && !connected.has(id);
  const edgeActive = (e: GEdge) => !!focus && (e.from === focus || e.to === focus);
  const edgeDim = (e: GEdge) => !!focus && !edgeActive(e);

  return (
    <Canvas
      camera={{ position: [0, 0, distance], fov: 55, near: 0.1, far: distance * 6 }}
      gl={{ antialias: true }}
      dpr={[1, 2]}
      style={{ background: backdrop }}
      onPointerMissed={() => onSelect('')}
    >
      <fog attach="fog" args={[backdrop, distance * 0.9, distance * 3.2]} />
      <ambientLight intensity={0.85} />
      <directionalLight position={[8, 12, 10]} intensity={1.5} />
      <directionalLight position={[-10, -6, -8]} intensity={0.5} />

      {edges.map((e) => {
        const from = positions.get(e.from);
        const to = positions.get(e.to);
        if (!from || !to) return null;
        const active = edgeActive(e);
        return (
          <Line
            key={e.id}
            points={[from, to]}
            color={active ? ACCENT : '#5a5a5a'}
            lineWidth={active ? 2 : 1}
            transparent
            opacity={edgeDim(e) ? 0.06 : active ? 0.95 : 0.4}
            {...(e.kind === 'derives' ? { dashed: true, dashSize: 0.45, gapSize: 0.35 } : {})}
          />
        );
      })}

      {nodes.map((n) => {
        const p = positions.get(n.id);
        if (!p) return null;
        // Memory is sized by what it holds; everything else by how connected it is.
        const r = n.kind === 'memory'
          ? 0.5 + Math.min(Math.sqrt(n.weight ?? 1), 6) * 0.18
          : 0.42 + Math.min(degree.get(n.id) ?? 0, 8) * 0.045;
        return (
          <Node
            key={n.id}
            node={n}
            position={p}
            radius={r}
            color={NODE_COLOR[n.kind]}
            labelColor={labelColor}
            dim={dimmed(n.id)}
            selected={selected === n.id}
            onHover={onHover}
            onSelect={onSelect}
          />
        );
      })}

      {/* Off by default: a moving scene makes picking out a node much harder. */}
      <OrbitControls makeDefault enableDamping dampingFactor={0.08} autoRotate={spin} autoRotateSpeed={0.35} minDistance={4} maxDistance={distance * 3} />
    </Canvas>
  );
}
