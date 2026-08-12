import type { ReactNode } from "react";

type Point = [number, number];

const diagrams = {
  "egraph-equivalence": {
    label: "Equivalent expressions collected in one e-class with proof reasons and extraction",
    caption: "Keep the alternatives. Record why they are equal. Choose only at extraction time.",
    note: "The important object is not an arrow from old program to new program. It is the set of forms we are still allowed to choose from.",
  },
  "compiler-territory": {
    label: "The EvoCC compiler territory from GNU C source to a bootable ARM64 kernel",
    caption: "The semantic path narrows as it approaches the machine; the proof path remains beside it.",
    note: "The assembler and linker may package EvoCC's decision. They may not make the semantic decision for it.",
  },
  "semantic-and-evidence": {
    label: "The semantic system and evidence system running in parallel",
    caption: "One system produces the artifact. The other makes the production claim auditable.",
    note: "Neither rail can certify the other by itself. The useful evidence appears where the two rails join.",
  },
  "three-arm64-worlds": {
    label: "Three ARM64 execution worlds on one Apple silicon development host",
    caption: "Build once under a sealed Linux world; observe each image under two independent QEMU execution modes.",
    note: "Same architecture does not mean same world. Every boundary needs an identity and an observable result.",
  },
  "nonce-bound-boot": {
    label: "Four stages of a nonce-bound kernel boot acceptance flight",
    caption: "A boot log becomes evidence only after it proves which flight produced it.",
    note: "The nonce is a causal join. Without it, a beautiful boot log may simply belong to yesterday's run.",
  },
} as const;

export type SketchName = keyof typeof diagrams;

function RoughPath({ d, arrow = false, dashed = false, className = "" }: {
  d: string;
  arrow?: boolean;
  dashed?: boolean;
  className?: string;
}) {
  const classes = `sketch-line${dashed ? " sketch-dashed" : ""} ${className}`.trim();
  return (
    <g className={classes}>
      <path className="sketch-line-echo" d={d} />
      <path d={d} markerEnd={arrow ? "url(#sketch-arrow)" : undefined} />
    </g>
  );
}

function RoughBox({ x, y, width, height, children, accent = false, tilt = 0 }: {
  x: number;
  y: number;
  width: number;
  height: number;
  children?: ReactNode;
  accent?: boolean;
  tilt?: number;
}) {
  const d = `M ${x + 3} ${y + 1} Q ${x + width / 2} ${y - 2} ${x + width - 2} ${y + 2}
    Q ${x + width + 2} ${y + height / 2} ${x + width - 1} ${y + height - 2}
    Q ${x + width / 2} ${y + height + 2} ${x + 2} ${y + height - 1}
    Q ${x - 2} ${y + height / 2} ${x + 3} ${y + 1} Z`;
  return (
    <g transform={`rotate(${tilt} ${x + width / 2} ${y + height / 2})`}>
      <path className={accent ? "sketch-box sketch-box-accent" : "sketch-box"} d={d} />
      {children}
    </g>
  );
}

function Label({ x, y, children, anchor = "middle", className = "" }: {
  x: number;
  y: number;
  children: ReactNode;
  anchor?: "start" | "middle" | "end";
  className?: string;
}) {
  return <text x={x} y={y} textAnchor={anchor} className={`sketch-label ${className}`}>{children}</text>;
}

function MultilineLabel({ x, y, lines, anchor = "middle", className = "" }: {
  x: number;
  y: number;
  lines: string[];
  anchor?: "start" | "middle" | "end";
  className?: string;
}) {
  return (
    <text x={x} y={y} textAnchor={anchor} className={`sketch-label ${className}`}>
      {lines.map((line, index) => <tspan key={line} x={x} dy={index === 0 ? 0 : 20}>{line}</tspan>)}
    </text>
  );
}

function Dot({ x, y, accent = false, radius = 5 }: { x: number; y: number; accent?: boolean; radius?: number }) {
  return <circle cx={x} cy={y} r={radius} className={accent ? "sketch-dot sketch-dot-accent" : "sketch-dot"} />;
}

function Paper({ x, y, label, tilt = -2 }: { x: number; y: number; label: string; tilt?: number }) {
  return (
    <RoughBox x={x} y={y} width={112} height={82} tilt={tilt}>
      <path className="sketch-fine" d={`M ${x + 17} ${y + 21} H ${x + 92} M ${x + 17} ${y + 34} H ${x + 82} M ${x + 17} ${y + 47} H ${x + 95}`} />
      <Label x={x + 56} y={y + 68} className="sketch-small">{label}</Label>
    </RoughBox>
  );
}

function Tree({ x, y }: { x: number; y: number }) {
  const nodes: Point[] = [[x, y], [x - 48, y + 58], [x + 48, y + 58], [x - 73, y + 116], [x - 22, y + 116], [x + 48, y + 116]];
  return (
    <g>
      <RoughPath d={`M ${x} ${y + 8} L ${x - 48} ${y + 50} M ${x} ${y + 8} L ${x + 48} ${y + 50} M ${x - 48} ${y + 66} L ${x - 73} ${y + 108} M ${x - 48} ${y + 66} L ${x - 22} ${y + 108} M ${x + 48} ${y + 66} V ${y + 108}`} />
      {nodes.map(([cx, cy], index) => <Dot key={`${cx}-${cy}`} x={cx} y={cy} accent={index === 0} radius={index < 3 ? 8 : 6} />)}
    </g>
  );
}

function Network({ x, y, scale = 1 }: { x: number; y: number; scale?: number }) {
  const points: Point[] = [[0, 25], [45, 0], [85, 32], [20, 78], [66, 83], [108, 74], [44, 126], [96, 128]];
  const edges = [[0,1],[0,3],[1,2],[1,4],[2,4],[2,5],[3,4],[3,6],[4,5],[4,6],[4,7],[5,7],[6,7]];
  return (
    <g transform={`translate(${x} ${y}) scale(${scale})`}>
      {edges.map(([a, b]) => <RoughPath key={`${a}-${b}`} d={`M ${points[a][0]} ${points[a][1]} L ${points[b][0]} ${points[b][1]}`} />)}
      {points.map(([cx, cy], index) => <Dot key={`${cx}-${cy}`} x={cx} y={cy} accent={index === 4} radius={5.5} />)}
    </g>
  );
}

function EgraphEquivalence() {
  return (
    <svg viewBox="0 0 900 430" role="img">
      <SketchDefs />
      <title>Equivalent forms are collected before cost-guided extraction</title>
      <path className="sketch-wash" d="M295 82 Q443 18 591 82 Q669 170 590 271 Q444 328 297 271 Q220 169 295 82Z" />
      <RoughPath d="M 445 74 C 354 71 283 118 272 185 C 261 251 335 295 445 302 C 555 295 629 251 618 185 C 607 118 536 71 445 74 Z" />
      <Label x={445} y={181} className="sketch-display">one e-class</Label>
      <Label x={445} y={212} className="sketch-muted">all forms known equal</Label>

      <RoughBox x={42} y={68} width={151} height={68} tilt={-2}>
        <MultilineLabel x={117} y={94} lines={["add %x, 0", "rewrite form"]} className="sketch-small" />
      </RoughBox>
      <RoughBox x={47} y={260} width={140} height={62} tilt={2} accent>
        <MultilineLabel x={117} y={286} lines={["%x", "cheapest form"]} className="sketch-small" />
      </RoughBox>
      <RoughBox x={698} y={76} width={160} height={72} tilt={1}>
        <MultilineLabel x={778} y={102} lines={["guarded form", "premises attached"]} className="sketch-small" />
      </RoughBox>
      <RoughBox x={700} y={257} width={154} height={70} tilt={-1}>
        <MultilineLabel x={777} y={283} lines={["target form", "cost = 3"]} className="sketch-small" />
      </RoughBox>

      <RoughPath d="M 193 103 C 246 109 268 132 302 158" arrow />
      <RoughPath d="M 187 286 C 241 274 272 251 307 230" arrow />
      <RoughPath d="M 698 111 C 650 123 626 145 590 166" arrow />
      <RoughPath d="M 596 238 C 644 256 664 281 699 290" arrow />

      <RoughBox x={287} y={352} width={316} height={50} tilt={-1}>
        <Label x={445} y={382} className="sketch-small">proof ledger · rule · substitution · premises</Label>
      </RoughBox>
      <RoughPath d="M 352 286 C 348 317 358 332 379 351" arrow dashed />
      <RoughPath d="M 540 286 C 546 319 535 334 515 352" arrow dashed />
      <Label x={676} y={373} anchor="start" className="sketch-annotation">extract once, at the end →</Label>
    </svg>
  );
}

function CompilerTerritory() {
  return (
    <svg viewBox="0 0 980 520" role="img">
      <SketchDefs />
      <title>The EvoCC compiler path and its parallel proof path</title>
      <Paper x={34} y={58} label="GNU C11" />
      <RoughPath d="M 146 100 C 174 100 184 100 208 100" arrow />

      <circle cx="260" cy="102" r="55" className="sketch-box" />
      {[0,45,90,135].map((angle) => <line key={angle} x1="260" y1="102" x2={260 + 73 * Math.cos(angle * Math.PI / 180)} y2={102 + 73 * Math.sin(angle * Math.PI / 180)} className="sketch-fine" />)}
      <Label x={260} y={98}>EvoCPP</Label><Label x={260} y={120} className="sketch-small">tokens</Label>

      <RoughPath d="M 319 105 C 355 107 366 126 390 146" arrow />
      <Tree x={448} y={83} />
      <Label x={448} y={225} className="sketch-small">parser · C semantics</Label>

      <RoughPath d="M 503 177 C 548 185 558 208 580 232" arrow />
      <RoughBox x={575} y={214} width={150} height={106} tilt={-1} accent>
        <path className="sketch-fine" d="M595 246 H704 M595 267 H686 M595 288 H697" />
        <Label x={650} y={310} className="sketch-small">C-HIR → EvoIR</Label>
      </RoughBox>

      <RoughPath d="M 727 270 C 755 269 765 253 781 235" arrow />
      <Network x={788} y={103} scale={0.88} />
      <Label x={838} y={260} className="sketch-small">e-graph</Label>

      <RoughPath d="M 838 280 C 822 327 775 349 724 363" arrow />
      <path className="sketch-box sketch-box-accent" d="M646 340 L758 340 L724 407 L681 407 Z" />
      <Label x={702} y={371} className="sketch-small">extract</Label>
      <Label x={702} y={393} className="sketch-small">EvoMIR</Label>

      <RoughPath d="M 648 385 C 602 397 573 410 548 430" arrow />
      <RoughBox x={389} y={397} width={158} height={82} tilt={1}>
        <MultilineLabel x={468} y={428} lines={["assembly", "+ linker"]} />
      </RoughBox>
      <RoughPath d="M 389 441 C 343 442 319 427 289 408" arrow />
      <RoughBox x={121} y={366} width={169} height={106} tilt={-1} accent>
        <path className="sketch-fine" d="M145 391 H267 V435 H145 Z M168 446 H244" />
        <MultilineLabel x={206} y={415} lines={["ARM64", "Linux Image"]} className="sketch-small" />
      </RoughBox>

      <RoughPath d="M 804 286 C 796 340 814 389 846 421" dashed arrow />
      <RoughBox x={789} y={418} width={155} height={60} tilt={2}>
        <MultilineLabel x={866} y={442} lines={["proof artifact", "independent replay"]} className="sketch-small" />
      </RoughBox>
      <Label x={42} y={512} anchor="start" className="sketch-annotation">semantic detail decreases → machine commitment increases</Label>
    </svg>
  );
}

function SemanticAndEvidence() {
  const semantic = ["source", "C-HIR", "EvoIR", "e-graph", "machine code"];
  const evidence = ["contracts", "tool identity", "observations", "predicates", "sealed evidence"];
  return (
    <svg viewBox="0 0 920 430" role="img">
      <SketchDefs />
      <title>The semantic and evidence systems are separate rails with explicit joins</title>
      <path className="sketch-wash" d="M32 72 Q450 39 886 68 L884 180 Q455 210 34 177Z" />
      <path className="sketch-wash sketch-wash-green" d="M32 267 Q450 238 887 264 L885 392 Q455 419 34 391Z" />
      <Label x={54} y={55} anchor="start" className="sketch-annotation">the thing we build</Label>
      <Label x={54} y={252} anchor="start" className="sketch-annotation">why we believe this run</Label>
      <RoughPath d="M 80 139 C 275 125 615 145 842 130" />
      <RoughPath d="M 80 333 C 298 318 620 343 842 326" />
      {semantic.map((label, index) => {
        const x = 90 + index * 187;
        return <g key={label}><Dot x={x} y={134} accent={index === 3} radius={11} /><Label x={x} y={104} className="sketch-small">{label}</Label></g>;
      })}
      {evidence.map((label, index) => {
        const x = 90 + index * 187;
        return <g key={label}><Dot x={x} y={329} accent={index === 4} radius={11} /><Label x={x} y={370} className="sketch-small">{label}</Label></g>;
      })}
      <RoughPath d="M 90 317 C 89 258 90 213 90 149" dashed arrow />
      <RoughPath d="M 277 317 C 350 272 606 214 838 148" dashed arrow />
      <RoughPath d="M 277 151 C 335 207 430 251 464 316" dashed arrow />
      <RoughPath d="M 651 151 C 646 213 635 257 651 316" dashed arrow />
      <RoughPath d="M 838 150 C 789 206 744 264 838 315" dashed arrow />
      <Label x={95} y={218} anchor="start" className="sketch-tiny">constrains</Label>
      <Label x={444} y={236} className="sketch-tiny">spans · reasons · logs</Label>
      <Label x={827} y={232} anchor="end" className="sketch-tiny">executes + observes</Label>
    </svg>
  );
}

function ThreeArm64Worlds() {
  return (
    <svg viewBox="0 0 980 520" role="img">
      <SketchDefs />
      <title>Apple silicon host, Linux build VM, and two QEMU boot modes</title>
      <path className="sketch-ground" d="M25 470 C165 446 305 487 461 462 C617 438 762 482 953 455" />
      <RoughBox x={31} y={45} width={918} height={422} tilt={0}>
        <Label x={72} y={78} anchor="start" className="sketch-annotation">macOS · Apple silicon · the observing host</Label>
      </RoughBox>

      <RoughBox x={64} y={120} width={245} height={270} tilt={-1} accent>
        <MultilineLabel x={186} y={151} lines={["sealed Linux", "build micro-VM"]} />
        <Paper x={98} y={181} label="6.18.44" tilt={-3} />
        <RoughBox x={218} y={190} width={66} height={50}><Label x={251} y={220} className="sketch-small">GCC</Label></RoughBox>
        <RoughBox x={218} y={264} width={66} height={50}><Label x={251} y={294} className="sketch-small">Clang</Label></RoughBox>
        <Label x={186} y={355} className="sketch-small">two independent Images</Label>
      </RoughBox>

      <RoughPath d="M 310 250 C 362 221 385 192 418 177" arrow />
      <RoughPath d="M 310 283 C 361 311 386 336 418 350" arrow />
      <RoughBox x={419} y={104} width={211} height={160} tilt={1}>
        <Label x={525} y={139}>QEMU + HVF</Label>
        <path className="sketch-fine" d="M457 169 H593 V225 H457 Z M483 239 H566" />
        <Label x={525} y={196} className="sketch-small">CPU = host</Label>
      </RoughBox>
      <RoughBox x={419} y={286} width={211} height={160} tilt={-1}>
        <Label x={525} y={321}>QEMU + TCG</Label>
        <path className="sketch-fine" d="M457 351 H593 V407 H457 Z M483 421 H566" />
        <Label x={525} y={378} className="sketch-small">cortex-a72</Label>
      </RoughBox>

      <RoughPath d="M 631 184 C 679 184 691 190 717 209" arrow />
      <RoughPath d="M 631 366 C 680 361 692 340 718 324" arrow />
      <RoughBox x={718} y={165} width={192} height={218} tilt={1} accent>
        <Label x={814} y={201}>serial evidence</Label>
        <path className="sketch-fine" d="M747 224 H880 M747 248 H866 M747 272 H884 M747 296 H845 M747 320 H875" />
        <Dot x={752} y={347} accent radius={7} />
        <Label x={770} y={352} anchor="start" className="sketch-small">fresh nonce found</Label>
      </RoughBox>
      <Label x={334} y={493} className="sketch-annotation">build identity</Label>
      <Label x={621} y={493} className="sketch-annotation">execution diversity</Label>
      <Label x={844} y={493} className="sketch-annotation">joined observation</Label>
    </svg>
  );
}

function BootPanel({ x, letter, stage, lines, active = 0 }: { x: number; letter: string; stage: string; lines: string[]; active?: number }) {
  return (
    <g>
      <Label x={x} y={45} anchor="start" className="sketch-display">{letter}.</Label>
      <RoughBox x={x} y={68} width={188} height={255} tilt={letter === "B" ? -1 : 1}>
        <Label x={x + 94} y={100} className="sketch-small">{stage}</Label>
        <path className="sketch-fine" d={`M ${x + 26} 122 H ${x + 161} V 281 H ${x + 26} Z`} />
        {lines.map((line, index) => (
          <g key={line}>
            <Dot x={x + 48} y={157 + index * 40} radius={7} accent={index < active} />
            <Label x={x + 67} y={162 + index * 40} anchor="start" className="sketch-tiny">{line}</Label>
          </g>
        ))}
      </RoughBox>
    </g>
  );
}

function NonceBoundBoot() {
  const lines = ["ALIVE", "NONCE N", "DONE"];
  return (
    <svg viewBox="0 0 980 410" role="img">
      <SketchDefs />
      <title>A nonce-bound boot is accepted in four causally joined stages</title>
      <BootPanel x={18} letter="A" stage="inject N" lines={lines} active={0} />
      <BootPanel x={263} letter="B" stage="init starts" lines={lines} active={1} />
      <BootPanel x={508} letter="C" stage="join this run" lines={lines} active={2} />
      <BootPanel x={753} letter="D" stage="power off" lines={lines} active={3} />
      <RoughPath d="M 208 196 C 230 181 241 181 261 196" arrow />
      <RoughPath d="M 453 196 C 475 181 486 181 506 196" arrow />
      <RoughPath d="M 698 196 C 720 181 731 181 751 196" arrow />
      <Label x={112} y={365} className="sketch-annotation">boot Image + N</Label>
      <Label x={357} y={365} className="sketch-annotation">guest speaks</Label>
      <Label x={602} y={365} className="sketch-annotation">exactly once</Label>
      <Label x={847} y={365} className="sketch-annotation">controlled exit</Label>
      <RoughPath d="M 771 384 C 817 403 879 402 928 377" arrow />
      <Label x={753} y={400} anchor="start" className="sketch-tiny">only now: PASS</Label>
    </svg>
  );
}

function SketchDefs() {
  return (
    <defs>
      <marker id="sketch-arrow" viewBox="0 0 10 10" refX="8" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse">
        <path d="M 1 1 L 9 5 L 1 9" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round" />
      </marker>
      <filter id="paper-wobble" x="-3%" y="-3%" width="106%" height="106%">
        <feTurbulence type="fractalNoise" baseFrequency="0.012 0.06" numOctaves="1" seed="17" result="noise" />
        <feDisplacementMap in="SourceGraphic" in2="noise" scale="1.25" />
      </filter>
    </defs>
  );
}

function Drawing({ name }: { name: SketchName }) {
  switch (name) {
    case "egraph-equivalence": return <EgraphEquivalence />;
    case "compiler-territory": return <CompilerTerritory />;
    case "semantic-and-evidence": return <SemanticAndEvidence />;
    case "three-arm64-worlds": return <ThreeArm64Worlds />;
    case "nonce-bound-boot": return <NonceBoundBoot />;
  }
}

export function SketchDiagram({ source }: { source: string }) {
  const name = source.trim() as SketchName;
  const diagram = diagrams[name];
  if (!diagram) {
    return <pre className="sketch-unknown"><code>Unknown EvoCC sketch: {source}</code></pre>;
  }

  return (
    <figure className="sketch-figure" data-sketch={name} aria-label={diagram.label}>
      <div className="sketch-canvas"><Drawing name={name} /></div>
      <div className="sketch-words">
        <figcaption>{diagram.caption}</figcaption>
        <aside>{diagram.note}</aside>
      </div>
    </figure>
  );
}
