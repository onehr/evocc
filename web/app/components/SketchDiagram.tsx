import type { ReactNode } from "react";
import type { NotebookLocale } from "../generated/notebook";

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
  "translation-phases": {
    label: "C source passing through physical-line repair, tokenization, directives, macro expansion, and parser handoff",
    caption: "The preprocessor is a staged transducer: each stage changes which distinctions still exist.",
    note: "A later stage cannot recover a boundary that an earlier stage erased. This is why splicing, tokenization, expansion, and rendering must have separate contracts.",
  },
  "token-anatomy": {
    label: "An EvoCC preprocessing token joined to spelling storage and a stable source span",
    caption: "Kind answers what the token is; spelling answers what was written; the span answers where the claim came from.",
    note: "Borrowed spelling is reconstructed from one source file. Decoded spelling owns generated or cross-file text. Confusing them produces precise, repeatable crashes.",
  },
  "maximal-munch": {
    label: "Four token-boundary examples showing why preprocessed text must not fuse adjacent punctuators",
    caption: "Equal characters are not enough. The renderer must preserve the token sequence when its output is lexed again.",
    note: "Whitespace is optional only when concatenation round-trips to the same token kinds. Otherwise one space is semantic armor.",
  },
  "macro-rescan": {
    label: "Function-like macro argument capture, prescan, substitution, disabled-set protection, and rescan",
    caption: "Macro expansion is a small evaluator with state, not global search-and-replace.",
    note: "The disabled set breaks direct and indirect recursion. Rescanning is what lets replacement text expose another macro call without expanding forever.",
  },
  "hash-and-paste": {
    label: "Stringification and token pasting following distinct argument-expansion paths",
    caption: "The two sharpest macro operators deliberately bypass ordinary argument expansion—and then demand different validation.",
    note: "Stringification preserves the argument's token spelling. Pasting concatenates spellings and must re-lex to exactly one preprocessing token.",
  },
  "conditional-stack": {
    label: "Nested conditional-compilation frames tracking active branches and prior true branches",
    caption: "A directive is interpreted even when its ordinary tokens are suppressed, because it changes the stack that decides future visibility.",
    note: "The current line is emitted only when every enclosing frame is active. had_true prevents a later #elif or #else from becoming active twice.",
  },
  "include-source-graph": {
    label: "A translation unit expanded into a graph of source files with stable file identities and owned cross-file token spellings",
    caption: "An include does not paste anonymous bytes. It introduces another source identity into the same translation unit.",
    note: "FileId keeps diagnostics attached to the correct file; Decoded token text prevents a header span from being sliced out of the parent source buffer.",
  },
  "differential-oracle": {
    label: "EvoCC and GCC preprocessing the same frozen corpus before an independent token serializer compares results",
    caption: "Compare preprocessing tokens, exit status, and controlled diagnostics—not incidental whitespace.",
    note: "The denominator is part of the claim: exactly 34 fixtures must be accounted for, and every one must be PASS. Missing, skipped, or conflicting cases fail the gate.",
  },
} as const;

const chineseDiagramCopy: Record<SketchName, { label: string; caption: string; note: string }> = {
  "egraph-equivalence": {
    label: " e-class, , ",
    caption: ", , . ",
    note: ", . ",
  },
  "compiler-territory": {
    label: "EvoCC GNU C ARM64 ",
    caption: ", . ",
    note: "assembler linker EvoCC , EvoCC . ",
  },
  "semantic-and-evidence": {
    label: ", ",
    caption: ", . ",
    note: "; . ",
  },
  "three-arm64-worlds": {
    label: " Apple silicon ARM64 ",
    caption: " Linux , QEMU Image. ",
    note: ". . ",
  },
  "nonce-bound-boot": {
    label: "nonce-bound kernel boot acceptance flight ",
    caption: " flight , boot log . ",
    note: "nonce . , boot log . ",
  },
  "translation-phases": {
    label: "C , tokenize, directive, parser ",
    caption: " transducer; . ",
    note: ". splice, tokenize, expand render . ",
  },
  "token-anatomy": {
    label: " EvoCC preprocessing token spelling storage source span",
    caption: "kind , spelling , span . ",
    note: "Borrowed spelling ; Decoded spelling . . ",
  },
  "maximal-munch": {
    label: " token boundary punctuator ",
    caption: "; lex , token sequence. ",
    note: " round-trip token kinds , ; . ",
  },
  "macro-rescan": {
    label: "function-like macro , , , disabled-set ",
    caption: " evaluator, . ",
    note: "disabled set ; rescan replacement text macro call, . ",
  },
  "hash-and-paste": {
    label: "stringification token pasting ",
    caption: " macro operator , . ",
    note: "Stringification token spelling; pasting spelling, lex preprocessing token. ",
  },
  "conditional-stack": {
    label: " conditional compilation frame active branch prior true branch",
    caption: " token , directive , stack. ",
    note: " frame active , ; had_true #elif #else . ",
  },
  "include-source-graph": {
    label: "translation unit file identity spelling source graph",
    caption: "include ; translation unit source identity. ",
    note: "FileId ; Decoded token text header span parent source buffer. ",
  },
  "differential-oracle": {
    label: "EvoCC GCC corpus, token serializer ",
    caption: " preprocessing token, exit status , . ",
    note: "denominator : 34 fixture, PASS; , gate . ",
  },
};

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

function EgraphEquivalence({ locale }: { locale: NotebookLocale }) {
  const zh = locale === "zh";
  return (
    <svg viewBox="0 0 900 430" role="img">
      <SketchDefs />
      <title>{zh ? ", " : "Equivalent forms are collected before cost-guided extraction"}</title>
      <path className="sketch-wash" d="M295 82 Q443 18 591 82 Q669 170 590 271 Q444 328 297 271 Q220 169 295 82Z" />
      <RoughPath d="M 445 74 C 354 71 283 118 272 185 C 261 251 335 295 445 302 C 555 295 629 251 618 185 C 607 118 536 71 445 74 Z" />
      <Label x={445} y={181} className="sketch-display">{zh ? " e-class" : "one e-class"}</Label>
      <Label x={445} y={212} className="sketch-muted">{zh ? "" : "all forms known equal"}</Label>

      <RoughBox x={42} y={68} width={151} height={68} tilt={-2}>
        <MultilineLabel x={117} y={94} lines={["add %x, 0", zh ? "rewrite " : "rewrite form"]} className="sketch-small" />
      </RoughBox>
      <RoughBox x={47} y={260} width={140} height={62} tilt={2} accent>
        <MultilineLabel x={117} y={286} lines={["%x", zh ? "" : "cheapest form"]} className="sketch-small" />
      </RoughBox>
      <RoughBox x={698} y={76} width={160} height={72} tilt={1}>
        <MultilineLabel x={778} y={102} lines={zh ? [" guard ", ""] : ["guarded form", "premises attached"]} className="sketch-small" />
      </RoughBox>
      <RoughBox x={700} y={257} width={154} height={70} tilt={-1}>
        <MultilineLabel x={777} y={283} lines={[zh ? "" : "target form", "cost = 3"]} className="sketch-small" />
      </RoughBox>

      <RoughPath d="M 193 103 C 246 109 268 132 302 158" arrow />
      <RoughPath d="M 187 286 C 241 274 272 251 307 230" arrow />
      <RoughPath d="M 698 111 C 650 123 626 145 590 166" arrow />
      <RoughPath d="M 596 238 C 644 256 664 281 699 290" arrow />

      <RoughBox x={287} y={352} width={316} height={50} tilt={-1}>
        <Label x={445} y={382} className="sketch-small">{zh ? " · · · " : "proof ledger · rule · substitution · premises"}</Label>
      </RoughBox>
      <RoughPath d="M 352 286 C 348 317 358 332 379 351" arrow dashed />
      <RoughPath d="M 540 286 C 546 319 535 334 515 352" arrow dashed />
      <Label x={676} y={373} anchor="start" className="sketch-annotation">{zh ? " →" : "extract once, at the end →"}</Label>
    </svg>
  );
}

function CompilerTerritory({ locale }: { locale: NotebookLocale }) {
  const zh = locale === "zh";
  return (
    <svg viewBox="0 0 980 520" role="img">
      <SketchDefs />
      <title>{zh ? "EvoCC " : "The EvoCC compiler path and its parallel proof path"}</title>
      <Paper x={34} y={58} label="GNU C11" />
      <RoughPath d="M 146 100 C 174 100 184 100 208 100" arrow />

      <circle cx="260" cy="102" r="55" className="sketch-box" />
      {[0,45,90,135].map((angle) => <line key={angle} x1="260" y1="102" x2={260 + 73 * Math.cos(angle * Math.PI / 180)} y2={102 + 73 * Math.sin(angle * Math.PI / 180)} className="sketch-fine" />)}
      <Label x={260} y={98}>EvoCPP</Label><Label x={260} y={120} className="sketch-small">tokens</Label>

      <RoughPath d="M 319 105 C 355 107 366 126 390 146" arrow />
      <Tree x={448} y={83} />
      <Label x={448} y={225} className="sketch-small">{zh ? "parser · C " : "parser · C semantics"}</Label>

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
        <MultilineLabel x={468} y={428} lines={zh ? ["", "+ linker"] : ["assembly", "+ linker"]} />
      </RoughBox>
      <RoughPath d="M 389 441 C 343 442 319 427 289 408" arrow />
      <RoughBox x={121} y={366} width={169} height={106} tilt={-1} accent>
        <path className="sketch-fine" d="M145 391 H267 V435 H145 Z M168 446 H244" />
        <MultilineLabel x={206} y={415} lines={["ARM64", "Linux Image"]} className="sketch-small" />
      </RoughBox>

      <RoughPath d="M 804 286 C 796 340 814 389 846 421" dashed arrow />
      <RoughBox x={789} y={418} width={155} height={60} tilt={2}>
        <MultilineLabel x={866} y={442} lines={zh ? [" artifact", " replay"] : ["proof artifact", "independent replay"]} className="sketch-small" />
      </RoughBox>
      <Label x={42} y={512} anchor="start" className="sketch-annotation">{zh ? " → " : "semantic detail decreases → machine commitment increases"}</Label>
    </svg>
  );
}

function SemanticAndEvidence({ locale }: { locale: NotebookLocale }) {
  const zh = locale === "zh";
  const semantic = zh ? ["", "C-HIR", "EvoIR", "e-graph", ""] : ["source", "C-HIR", "EvoIR", "e-graph", "machine code"];
  const evidence = zh ? ["", "", "", "", ""] : ["contracts", "tool identity", "observations", "predicates", "sealed evidence"];
  return (
    <svg viewBox="0 0 920 430" role="img">
      <SketchDefs />
      <title>{zh ? "" : "The semantic and evidence systems are separate rails with explicit joins"}</title>
      <path className="sketch-wash" d="M32 72 Q450 39 886 68 L884 180 Q455 210 34 177Z" />
      <path className="sketch-wash sketch-wash-green" d="M32 267 Q450 238 887 264 L885 392 Q455 419 34 391Z" />
      <Label x={54} y={55} anchor="start" className="sketch-annotation">{zh ? "" : "the thing we build"}</Label>
      <Label x={54} y={252} anchor="start" className="sketch-annotation">{zh ? "" : "why we believe this run"}</Label>
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
      <Label x={95} y={218} anchor="start" className="sketch-tiny">{zh ? "" : "constrains"}</Label>
      <Label x={444} y={236} className="sketch-tiny">{zh ? "spans · · logs" : "spans · reasons · logs"}</Label>
      <Label x={827} y={232} anchor="end" className="sketch-tiny">{zh ? " + " : "executes + observes"}</Label>
    </svg>
  );
}

function ThreeArm64Worlds({ locale }: { locale: NotebookLocale }) {
  const zh = locale === "zh";
  return (
    <svg viewBox="0 0 980 520" role="img">
      <SketchDefs />
      <title>{zh ? "Apple silicon , Linux VM QEMU " : "Apple silicon host, Linux build VM, and two QEMU boot modes"}</title>
      <path className="sketch-ground" d="M25 470 C165 446 305 487 461 462 C617 438 762 482 953 455" />
      <RoughBox x={31} y={45} width={918} height={422} tilt={0}>
        <Label x={72} y={78} anchor="start" className="sketch-annotation">{zh ? "macOS · Apple silicon · " : "macOS · Apple silicon · the observing host"}</Label>
      </RoughBox>

      <RoughBox x={64} y={120} width={245} height={270} tilt={-1} accent>
        <MultilineLabel x={186} y={151} lines={zh ? [" Linux", " micro-VM"] : ["sealed Linux", "build micro-VM"]} />
        <Paper x={98} y={181} label="6.18.44" tilt={-3} />
        <RoughBox x={218} y={190} width={66} height={50}><Label x={251} y={220} className="sketch-small">GCC</Label></RoughBox>
        <RoughBox x={218} y={264} width={66} height={50}><Label x={251} y={294} className="sketch-small">Clang</Label></RoughBox>
        <Label x={186} y={355} className="sketch-small">{zh ? " Image" : "two independent Images"}</Label>
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
        <Label x={814} y={201}>{zh ? "" : "serial evidence"}</Label>
        <path className="sketch-fine" d="M747 224 H880 M747 248 H866 M747 272 H884 M747 296 H845 M747 320 H875" />
        <Dot x={752} y={347} accent radius={7} />
        <Label x={770} y={352} anchor="start" className="sketch-small">{zh ? " fresh nonce" : "fresh nonce found"}</Label>
      </RoughBox>
      <Label x={334} y={493} className="sketch-annotation">{zh ? "" : "build identity"}</Label>
      <Label x={621} y={493} className="sketch-annotation">{zh ? "" : "execution diversity"}</Label>
      <Label x={844} y={493} className="sketch-annotation">{zh ? "" : "joined observation"}</Label>
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

function NonceBoundBoot({ locale }: { locale: NotebookLocale }) {
  const zh = locale === "zh";
  const lines = ["ALIVE", "NONCE N", "DONE"];
  return (
    <svg viewBox="0 0 980 410" role="img">
      <SketchDefs />
      <title>{zh ? "nonce-bound boot " : "A nonce-bound boot is accepted in four causally joined stages"}</title>
      <BootPanel x={18} letter="A" stage={zh ? " N" : "inject N"} lines={lines} active={0} />
      <BootPanel x={263} letter="B" stage={zh ? "init " : "init starts"} lines={lines} active={1} />
      <BootPanel x={508} letter="C" stage={zh ? "" : "join this run"} lines={lines} active={2} />
      <BootPanel x={753} letter="D" stage={zh ? "" : "power off"} lines={lines} active={3} />
      <RoughPath d="M 208 196 C 230 181 241 181 261 196" arrow />
      <RoughPath d="M 453 196 C 475 181 486 181 506 196" arrow />
      <RoughPath d="M 698 196 C 720 181 731 181 751 196" arrow />
      <Label x={112} y={365} className="sketch-annotation">{zh ? " Image + N" : "boot Image + N"}</Label>
      <Label x={357} y={365} className="sketch-annotation">{zh ? "guest " : "guest speaks"}</Label>
      <Label x={602} y={365} className="sketch-annotation">{zh ? "" : "exactly once"}</Label>
      <Label x={847} y={365} className="sketch-annotation">{zh ? "" : "controlled exit"}</Label>
      <RoughPath d="M 771 384 C 817 403 879 402 928 377" arrow />
      <Label x={753} y={400} anchor="start" className="sketch-tiny">{zh ? ": PASS" : "only now: PASS"}</Label>
    </svg>
  );
}

function TranslationPhases({ locale }: { locale: NotebookLocale }) {
  const zh = locale === "zh";
  const stages = zh
    ? [
        ["", "\\\\ + newline "],
        ["", "splice "],
        ["pp-tokens", ""],
        ["EvoCPP", "directive + macro"],
        ["parser tokens", ""],
      ]
    : [
        ["physical bytes", "\\\\ + newline still exists"],
        ["logical lines", "splices are gone"],
        ["pp-tokens", "boundaries are fixed"],
        ["EvoCPP", "directives + macros"],
        ["parser tokens", "syntax handoff"],
      ];
  const x = [30, 219, 408, 597, 786];
  return (
    <svg viewBox="0 0 980 470" role="img">
      <SketchDefs />
      <title>{zh ? "C parser " : "The preprocessing phases C source crosses before parsing"}</title>
      <path className="sketch-wash" d="M18 70 Q490 34 961 70 L958 258 Q491 289 20 255Z" />
      {stages.map(([title, detail], index) => (
        <g key={title}>
          <RoughBox x={x[index]} y={102} width={152} height={108} tilt={index % 2 === 0 ? -1 : 1} accent={index === 2 || index === 3}>
            <Label x={x[index] + 76} y={139} className="sketch-small">{title}</Label>
            <MultilineLabel x={x[index] + 76} y={170} lines={[detail]} className="sketch-tiny" />
            <Label x={x[index] + 76} y={195} className="sketch-muted">{`phase ${index + 1}`}</Label>
          </RoughBox>
          {index < stages.length - 1 && <RoughPath d={`M ${x[index] + 153} 155 C ${x[index] + 168} 142 ${x[index + 1] - 13} 142 ${x[index + 1]} 155`} arrow />}
        </g>
      ))}

      <Label x={28} y={312} anchor="start" className="sketch-annotation">{zh ? ", " : "each gate irreversibly removes distinctions"}</Label>
      <RoughPath d="M 54 341 C 258 328 723 329 920 343" />
      {[106, 295, 484, 673, 862].map((cx, index) => <Dot key={cx} x={cx} y={337 + (index % 2) * 4} accent={index === 2} radius={7} />)}
      <RoughBox x={205} y={383} width={565} height={56} tilt={-0.5}>
        <Label x={487} y={410} className="sketch-small">{zh ? ": FileId + byte span + token spelling" : "carried throughout: FileId + byte span + token spelling"}</Label>
        <Label x={487} y={429} className="sketch-tiny">{zh ? "" : "the source ledger survives textual transformation"}</Label>
      </RoughBox>
      <RoughPath d="M 484 355 L 484 382" dashed arrow />
    </svg>
  );
}

function TokenAnatomy({ locale }: { locale: NotebookLocale }) {
  const zh = locale === "zh";
  return (
    <svg viewBox="0 0 980 500" role="img">
      <SketchDefs />
      <title>{zh ? "preprocessing token kind, spelling source span" : "The kind, spelling, and source span of a preprocessing token"}</title>
      <RoughBox x={314} y={76} width={352} height={245} tilt={-1} accent>
        <Label x={490} y={112} className="sketch-display">Token</Label>
        <RoughBox x={347} y={137} width={286} height={43}><Label x={490} y={164} className="sketch-small">kind = Identifier</Label></RoughBox>
        <RoughBox x={347} y={195} width={286} height={43}><Label x={490} y={222} className="sketch-small">text = Borrowed | Decoded</Label></RoughBox>
        <RoughBox x={347} y={253} width={286} height={45}><Label x={490} y={280} className="sketch-small">span = (file, bytes, line, column)</Label></RoughBox>
      </RoughBox>

      <Paper x={35} y={103} label={zh ? "main.c " : "bytes in main.c"} tilt={-2} />
      <Label x={91} y={217} className="sketch-small">Borrowed</Label>
      <Label x={91} y={239} className="sketch-tiny">[120..125] → value</Label>
      <RoughPath d="M 148 144 C 215 145 253 174 346 211" arrow />

      <RoughBox x={39} y={307} width={194} height={83} tilt={2}>
        <MultilineLabel x={136} y={337} lines={zh ? ["Decoded(\"value\")", ""] : ["Decoded(\"value\")", "generated or cross-file"]} className="sketch-small" />
      </RoughBox>
      <RoughPath d="M 233 348 C 287 331 317 286 356 226" arrow />

      <RoughBox x={742} y={83} width={198} height={94} tilt={1}>
        <MultilineLabel x={841} y={112} lines={["FileId(7)", "bytes 48..53", "line 4 · column 9"]} className="sketch-small" />
      </RoughBox>
      <RoughPath d="M 666 270 C 706 247 730 211 783 176" arrow />
      <RoughBox x={742} y={245} width={198} height={131} tilt={-1}>
        <MultilineLabel x={841} y={276} lines={zh ? ["source table", "7 → include/x.h", "8 → main.c"] : ["source table", "7 → include/x.h", "8 → main.c"]} className="sketch-small" />
      </RoughBox>
      <RoughPath d="M 842 245 L 842 179" dashed arrow />

      <RoughBox x={298} y={417} width={384} height={52} tilt={1}>
        <Label x={490} y={448} className="sketch-small">{zh ? "invariant: span.file Borrowed text buffer" : "invariant: span.file selects the buffer for Borrowed text"}</Label>
      </RoughBox>
    </svg>
  );
}

function MaximalMunch({ locale }: { locale: NotebookLocale }) {
  const zh = locale === "zh";
  const rows = [
    { y: 108, wanted: "x  +  +  1", fused: "x++1", good: false },
    { y: 198, wanted: "x  -  -  1", fused: "x--1", good: false },
    { y: 288, wanted: "x  /  *  p", fused: "x/*p", good: false },
    { y: 378, wanted: ".  .  .", fused: "...", good: false },
  ];
  return (
    <svg viewBox="0 0 980 490" role="img">
      <SketchDefs />
      <title>{zh ? " token maximal-munch " : "Concatenating adjacent tokens can change maximal-munch tokenization"}</title>
      <Label x={48} y={58} anchor="start" className="sketch-annotation">{zh ? " token sequence" : "original token sequence"}</Label>
      <Label x={411} y={58} className="sketch-annotation">{zh ? "" : "concatenate without space"}</Label>
      <Label x={820} y={58} className="sketch-annotation">{zh ? " lex" : "lex again"}</Label>
      {rows.map((row, index) => (
        <g key={row.wanted}>
          <RoughBox x={35} y={row.y - 30} width={276} height={58} tilt={index % 2 ? 1 : -1}>
            <Label x={173} y={row.y + 6} className="sketch-small">{row.wanted}</Label>
          </RoughBox>
          <RoughPath d={`M 312 ${row.y} C 342 ${row.y - 13} 361 ${row.y - 13} 389 ${row.y}`} arrow />
          <RoughBox x={392} y={row.y - 30} width={178} height={58} accent>
            <Label x={481} y={row.y + 6} className="sketch-small">{row.fused}</Label>
          </RoughBox>
          <RoughPath d={`M 570 ${row.y} C 611 ${row.y - 10} 631 ${row.y - 10} 665 ${row.y}`} arrow />
          <RoughBox x={669} y={row.y - 30} width={265} height={58} tilt={index % 2 ? -1 : 1}>
            <Label x={801} y={row.y} className="sketch-small">{index === 0 ? "Identifier · ++ · Int" : index === 1 ? "Identifier · -- · Int" : index === 2 ? "Identifier · comment" : "Ellipsis"}</Label>
            <Label x={801} y={row.y + 19} className="sketch-tiny">{zh ? "≠ → " : "≠ original → insert a space"}</Label>
          </RoughBox>
        </g>
      ))}
      <Label x={484} y={470} className="sketch-annotation">{zh ? "renderer : lex(concat(tokens)) == token kinds ?" : "renderer predicate: lex(concat(tokens)) == token kinds ?"}</Label>
    </svg>
  );
}

function MacroRescan({ locale }: { locale: NotebookLocale }) {
  const zh = locale === "zh";
  const labels = zh
    ? ["", "", " TWICE", "", " ADD"]
    : ["recognize call", "capture argument", "substitute TWICE", "rescan output", "substitute ADD"];
  const values = ["TWICE(3)", "x = [3]", "ADD(3, 3)", "ADD is visible", "((3) + (3))"];
  const xs = [28, 218, 408, 598, 788];
  return (
    <svg viewBox="0 0 980 530" role="img">
      <SketchDefs />
      <title>{zh ? "macro expansion argument capture, substitution rescan" : "Macro argument capture, substitution, and rescan"}</title>
      <RoughBox x={55} y={42} width={370} height={74} tilt={-1}>
        <MultilineLabel x={240} y={70} lines={["#define ADD(a,b) ((a)+(b))", "#define TWICE(x) ADD(x,x)"]} className="sketch-small" />
      </RoughBox>
      <RoughBox x={566} y={42} width={350} height={74} tilt={1} accent>
        <Label x={741} y={71} className="sketch-small">{zh ? "disabled set: TWICE" : "disabled set: TWICE is expanding"}</Label>
        <Label x={741} y={94} className="sketch-tiny">{"{ TWICE }"}</Label>
      </RoughBox>
      {xs.map((x, index) => (
        <g key={labels[index]}>
          <RoughBox x={x} y={179} width={158} height={104} tilt={index % 2 === 0 ? -1 : 1} accent={index === 2 || index === 4}>
            <Label x={x + 79} y={211} className="sketch-tiny">{labels[index]}</Label>
            <MultilineLabel x={x + 79} y={246} lines={[values[index]]} className="sketch-small" />
          </RoughBox>
          {index < xs.length - 1 && <RoughPath d={`M ${x + 158} 231 C ${x + 173} 216 ${xs[index + 1] - 12} 216 ${xs[index + 1]} 231`} arrow />}
        </g>
      ))}
      <RoughPath d="M 486 284 C 488 322 455 348 393 360" dashed arrow />
      <RoughBox x={232} y={350} width={322} height={67} tilt={-1}>
        <Label x={393} y={378} className="sketch-small">{zh ? " rescan? " : "why rescan?"}</Label>
        <Label x={393} y={399} className="sketch-tiny">{zh ? "replacement text macro name" : "replacement text may reveal another macro name"}</Label>
      </RoughBox>
      <RoughPath d="M 735 284 C 733 336 686 365 625 381" dashed arrow />
      <RoughBox x={570} y={359} width={360} height={94} tilt={1}>
        <MultilineLabel x={750} y={387} lines={zh ? [" disabled set? ", "A→B, B→A ", ""] : ["why a disabled set?", "A→B and B→A must terminate", "the active name cannot re-enter"]} className="sketch-small" />
      </RoughBox>
      <Label x={487} y={497} className="sketch-annotation">{zh ? "capture → prescan → substitute → rescan, , " : "capture → prescan → substitute → rescan is semantic order, not implementation taste"}</Label>
    </svg>
  );
}

function HashAndPaste({ locale }: { locale: NotebookLocale }) {
  const zh = locale === "zh";
  return (
    <svg viewBox="0 0 980 520" role="img">
      <SketchDefs />
      <title>{zh ? " stringification token pasting " : "The distinct semantics of macro stringification and token pasting"}</title>
      <path className="sketch-wash" d="M25 53 Q240 31 474 55 L470 453 Q236 476 25 450Z" />
      <path className="sketch-wash sketch-wash-green" d="M509 55 Q746 28 957 56 L957 452 Q739 476 508 450Z" />
      <Label x={247} y={73} className="sketch-display"># stringify</Label>
      <Label x={733} y={73} className="sketch-display">## paste</Label>

      <RoughBox x={62} y={106} width={368} height={62} tilt={-1}><Label x={246} y={143} className="sketch-small">#define STR(x) #x</Label></RoughBox>
      <RoughBox x={87} y={206} width={318} height={58} accent><Label x={246} y={242} className="sketch-small">STR(VALUE + 1)</Label></RoughBox>
      <RoughPath d="M 246 168 L 246 205" arrow />
      <RoughBox x={87} y={307} width={318} height={69} tilt={1}>
        <MultilineLabel x={246} y={335} lines={["\"VALUE + 1\"", zh ? " spelling, " : "original spelling, space-normalized"]} className="sketch-small" />
      </RoughBox>
      <RoughPath d="M 246 264 L 246 306" arrow />
      <Label x={246} y={419} className="sketch-annotation">{zh ? "" : "argument is not prescanned"}</Label>

      <RoughBox x={548} y={106} width={368} height={62} tilt={1}><Label x={732} y={143} className="sketch-small">#define JOIN(a,b) a ## b</Label></RoughBox>
      <RoughBox x={573} y={206} width={318} height={58} accent><Label x={732} y={242} className="sketch-small">JOIN(var, 7)</Label></RoughBox>
      <RoughPath d="M 732 168 L 732 205" arrow />
      <RoughBox x={573} y={307} width={146} height={69} tilt={-1}><Label x={646} y={348} className="sketch-small">var7</Label></RoughBox>
      <RoughPath d="M 732 264 C 723 284 699 294 670 306" arrow />
      <RoughBox x={756} y={307} width={161} height={69} tilt={1} accent>
        <MultilineLabel x={837} y={334} lines={zh ? ["re-lex gate", " 1 token"] : ["re-lex gate", "exactly 1 token"]} className="sketch-small" />
      </RoughBox>
      <RoughPath d="M 720 342 C 735 342 743 342 755 342" arrow />
      <Label x={732} y={419} className="sketch-annotation">{zh ? "0 2+ tokens → E0106" : "0 or 2+ tokens → E0106"}</Label>
      <RoughPath d="M 480 95 V 446" dashed />
      <Label x={490} y={490} className="sketch-tiny">{zh ? " substitution" : "neither path is ordinary substitution"}</Label>
    </svg>
  );
}

function ConditionalStack({ locale }: { locale: NotebookLocale }) {
  const zh = locale === "zh";
  const source = ["#if OUTER", "  keep_A", "  #if INNER", "    maybe_B", "  #else", "    keep_C", "  #endif", "#endif"];
  return (
    <svg viewBox="0 0 980 530" role="img">
      <SketchDefs />
      <title>{zh ? "nested conditional directive active frame stack" : "How nested conditional directives change the active frame stack"}</title>
      <RoughBox x={37} y={53} width={285} height={411} tilt={-1}>
        <Label x={179} y={87} className="sketch-annotation">source</Label>
        {source.map((line, index) => <Label key={line} x={70} y={126 + index * 39} anchor="start" className={line.includes("keep") ? "sketch-small" : "sketch-tiny"}>{`${String(index + 1).padStart(2, "0")}  ${line}`}</Label>)}
      </RoughBox>
      <RoughPath d="M 323 121 C 361 123 378 123 412 128" arrow />
      <RoughPath d="M 323 200 C 361 203 379 204 412 211" arrow />
      <RoughPath d="M 323 277 C 360 283 378 297 412 312" arrow />
      <RoughPath d="M 323 355 C 361 355 378 356 412 360" arrow />

      <RoughBox x={414} y={76} width={238} height={91} accent>
        <MultilineLabel x={533} y={107} lines={["frame 1", "active=true", "had_true=true"]} className="sketch-small" />
      </RoughBox>
      <RoughBox x={414} y={185} width={238} height={108} tilt={1}>
        <MultilineLabel x={533} y={215} lines={["frame 2", "active=false", "had_true=false"]} className="sketch-small" />
        <Label x={533} y={271} className="sketch-tiny">{zh ? "maybe_B " : "maybe_B is suppressed"}</Label>
      </RoughBox>
      <RoughBox x={414} y={309} width={238} height={108} tilt={-1} accent>
        <MultilineLabel x={533} y={339} lines={["frame 2 after #else", "active=true", "had_true=true"]} className="sketch-small" />
        <Label x={533} y={395} className="sketch-tiny">{zh ? "keep_C " : "keep_C is emitted"}</Label>
      </RoughBox>

      <RoughBox x={721} y={110} width={214} height={271} tilt={1}>
        <Label x={828} y={145} className="sketch-annotation">{zh ? "" : "emission rule"}</Label>
        <MultilineLabel x={828} y={194} lines={zh ? ["emit(line) ⇔", " frame", " active"] : ["emit(line) ⇔", "every enclosing", "frame is active"]} className="sketch-small" />
        <RoughPath d="M 766 269 H 891" />
        <MultilineLabel x={828} y={304} lines={zh ? ["directive ", " stack "] : ["directives still execute", "or the stack cannot recover"]} className="sketch-small" />
      </RoughBox>
      <RoughPath d="M 653 242 C 687 242 698 229 721 218" arrow />
      <Label x={490} y={488} className="sketch-annotation">{zh ? "#endif , text line" : "#endif is a structural operation, not an ordinary text line"}</Label>
    </svg>
  );
}

function IncludeSourceGraph({ locale }: { locale: NotebookLocale }) {
  const zh = locale === "zh";
  return (
    <svg viewBox="0 0 980 550" role="img">
      <SketchDefs />
      <title>{zh ? "include graph token provenance" : "Source identity and token provenance across an include graph"}</title>
      <Paper x={38} y={76} label="main.c · FileId 1" />
      <Paper x={248} y={38} label="config.h · FileId 2" tilt={2} />
      <Paper x={248} y={211} label="types.h · FileId 3" tilt={-1} />
      <Paper x={451} y={128} label="local.h · FileId 4" tilt={1} />
      <RoughPath d="M 150 111 C 184 91 204 80 248 77" arrow />
      <RoughPath d="M 150 139 C 181 181 206 219 248 244" arrow />
      <RoughPath d="M 360 80 C 402 86 421 114 451 147" arrow />
      <Label x={184} y={78} className="sketch-tiny">{'"config.h"'}</Label>
      <Label x={180} y={223} className="sketch-tiny">&lt;types.h&gt;</Label>

      <RoughBox x={674} y={54} width={260} height={138} tilt={-1} accent>
        <MultilineLabel x={804} y={84} lines={zh ? ["include search order", "1. quoted sibling", "2. explicit -I", "3. standard roots"] : ["include search order", "1. quoted sibling", "2. explicit -I", "3. standard roots"]} className="sketch-small" />
        <Label x={804} y={173} className="sketch-tiny">-nostdinc ⇒ {zh ? " 3 " : "skip layer 3"}</Label>
      </RoughBox>
      <RoughPath d="M 557 142 C 601 129 629 112 674 104" arrow />

      <RoughBox x={45} y={376} width={385} height={111} tilt={1}>
        <MultilineLabel x={237} y={407} lines={zh ? ["header token", "span.file = FileId 3", "text = Decoded(\"size_t\")"] : ["header token", "span.file = FileId 3", "text = Decoded(\"size_t\")"]} className="sketch-small" />
      </RoughBox>
      <RoughPath d="M 304 293 C 300 328 283 350 258 376" dashed arrow />
      <RoughBox x={550} y={345} width={384} height={142} tilt={-1} accent>
        <Label x={742} y={378} className="sketch-annotation">{zh ? "" : "the prevented old failure"}</Label>
        <MultilineLabel x={742} y={410} lines={zh ? [" header byte span", " main.c buffer", "→ panic / spelling"] : ["slice main.c using a", "header byte span", "→ panic / wrong spelling"]} className="sketch-small" />
      </RoughBox>
      <RoughPath d="M 430 431 C 476 432 507 425 549 418" arrow />
      <Label x={490} y={524} className="sketch-tiny">{zh ? " translation unit source buffer, span FileId" : "one translation unit may have many buffers; every span belongs to exactly one FileId"}</Label>
    </svg>
  );
}

function DifferentialOracle({ locale }: { locale: NotebookLocale }) {
  const zh = locale === "zh";
  return (
    <svg viewBox="0 0 980 550" role="img">
      <SketchDefs />
      <title>{zh ? "34 fixture fail-closed differential oracle" : "The fail-closed differential oracle for 34 preprocessing fixtures"}</title>
      <RoughBox x={34} y={54} width={188} height={94} tilt={-1}>
        <MultilineLabel x={128} y={85} lines={zh ? [" corpus", "34 × .c", " argv"] : ["frozen corpus", "34 × .c", "same argv"]} className="sketch-small" />
      </RoughBox>
      <RoughPath d="M 222 97 C 260 90 285 76 318 73" arrow />
      <RoughPath d="M 222 107 C 266 132 286 173 318 204" arrow />
      <RoughBox x={320} y={39} width={215} height={92} tilt={1} accent>
        <MultilineLabel x={427} y={71} lines={["EvoCC -E", "stdout · stderr · rc"]} className="sketch-small" />
      </RoughBox>
      <RoughBox x={320} y={173} width={215} height={92} tilt={-1}>
        <MultilineLabel x={427} y={205} lines={["GCC 14 -E", "stdout · stderr · rc"]} className="sketch-small" />
      </RoughBox>

      <RoughBox x={634} y={87} width={295} height={122} tilt={1}>
        <Label x={781} y={120} className="sketch-annotation">{zh ? " serializer" : "independent serializer"}</Label>
        <MultilineLabel x={781} y={151} lines={zh ? ["", " kind + spelling", " punctuator "] : ["ignore incidental space", "keep kind + spelling", "longest punctuator first"]} className="sketch-small" />
      </RoughBox>
      <RoughPath d="M 535 84 C 576 88 595 112 634 131" arrow />
      <RoughPath d="M 535 220 C 581 214 599 185 634 166" arrow />

      <RoughBox x={235} y={343} width={510} height={116} tilt={-1} accent>
        <Label x={490} y={378} className="sketch-display">PASS ⇔</Label>
        <MultilineLabel x={490} y={410} lines={zh ? ["rc ∧ token streams ", "missing include: subject "] : ["same rc ∧ identical token streams", "missing include: both controlled-reject the same subject"]} className="sketch-small" />
      </RoughBox>
      <RoughPath d="M 781 210 C 750 275 682 314 626 343" arrow />
      <RoughBox x={32} y={481} width={916} height={48} tilt={1}>
        <Label x={490} y={511} className="sketch-small">{zh ? "overall PASS ⇔ total = 34 ∧ PASS = 34 ∧ non-PASS = 0" : "overall PASS ⇔ total = 34 ∧ PASS = 34 ∧ non-PASS = 0"}</Label>
      </RoughBox>
      <Label x={808} y={305} className="sketch-annotation">{zh ? "DIFFER / SKIP / CONFLICT → FAIL" : "DIFFER / SKIP / CONFLICT → FAIL"}</Label>
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

function Drawing({ name, locale }: { name: SketchName; locale: NotebookLocale }) {
  switch (name) {
    case "egraph-equivalence": return <EgraphEquivalence locale={locale} />;
    case "compiler-territory": return <CompilerTerritory locale={locale} />;
    case "semantic-and-evidence": return <SemanticAndEvidence locale={locale} />;
    case "three-arm64-worlds": return <ThreeArm64Worlds locale={locale} />;
    case "nonce-bound-boot": return <NonceBoundBoot locale={locale} />;
    case "translation-phases": return <TranslationPhases locale={locale} />;
    case "token-anatomy": return <TokenAnatomy locale={locale} />;
    case "maximal-munch": return <MaximalMunch locale={locale} />;
    case "macro-rescan": return <MacroRescan locale={locale} />;
    case "hash-and-paste": return <HashAndPaste locale={locale} />;
    case "conditional-stack": return <ConditionalStack locale={locale} />;
    case "include-source-graph": return <IncludeSourceGraph locale={locale} />;
    case "differential-oracle": return <DifferentialOracle locale={locale} />;
  }
}

export function SketchDiagram({ source, locale }: { source: string; locale: NotebookLocale }) {
  const name = source.trim() as SketchName;
  const diagram = locale === "zh" ? chineseDiagramCopy[name] : diagrams[name];
  if (!diagram) {
    return <pre className="sketch-unknown"><code>Unknown EvoCC sketch: {source}</code></pre>;
  }

  return (
    <figure className="sketch-figure" data-sketch={name} aria-label={diagram.label}>
      <div className="sketch-canvas"><Drawing name={name} locale={locale} /></div>
      <div className="sketch-words">
        <figcaption>{diagram.caption}</figcaption>
        <aside>{diagram.note}</aside>
      </div>
    </figure>
  );
}
