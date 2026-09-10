//! EvoMIR Section 6, "Records (revision 0.18; withdraws `E4011`)".
//!
//! "The machine applies AAPCS64 to those marks on both targets, so an EvoCC
//! object agrees with a GCC or clang object on either side of the call: a
//! `byval` record of at most 16 bytes is loaded from the copy into the next one
//! or two general registers as 8-byte chunks (a record whose alignment is 16
//! starts at an even register on Linux, as a `bits<128>` does) and, in the
//! callee, stored into a frame slot that becomes the parameter's place; a
//! `byval` record above 16 bytes stays a pointer to the caller's copy in the
//! next register or stack slot ... An `sret` result of at most 16 bytes is
//! loaded by the callee into `x0` and `x1` before `ret` and stored by the
//! caller into its destination; above 16 bytes the destination pointer travels
//! in `x8`."
//!
//! Every register assignment below was read off the oracles' assembly before it
//! was implemented; the sources and the output are under
//! `.evidence/builtin-probes/20260906-b10/`.

use std::process::Command;

use evocc_evoir::parse_section9_module;
use evocc_mir::{compile_module_for, compile_module_with, Target, Toolchain};

// ---------------------------------------------------------------------------
// The record shapes
// ---------------------------------------------------------------------------

/// One record: its C spelling, its layout, and the members a checksum reads.
struct Record {
    tag: &'static str,
    size: u64,
    align: u32,
    /// `(offset, width)` of every member, in bytes.
    members: &'static [(u64, u64)],
}

const RECORDS: &[Record] = &[
    Record {
        tag: "R4",
        size: 4,
        align: 4,
        members: &[(0, 4)],
    },
    // The zstd receipt's shape: `typedef struct { unsigned long long t; } T;`.
    Record {
        tag: "R8",
        size: 8,
        align: 8,
        members: &[(0, 8)],
    },
    Record {
        tag: "R12",
        size: 12,
        align: 4,
        members: &[(0, 4), (4, 4), (8, 4)],
    },
    Record {
        tag: "R16",
        size: 16,
        align: 8,
        members: &[(0, 8), (8, 8)],
    },
    // Size sixteen at alignment sixteen: the `__int128` member is what makes
    // Linux round the register number up to even and Darwin not.
    Record {
        tag: "R16A",
        size: 16,
        align: 16,
        members: &[(0, 8), (8, 8)],
    },
    Record {
        tag: "R24",
        size: 24,
        align: 8,
        members: &[(0, 8), (8, 8), (16, 8)],
    },
    Record {
        tag: "R32",
        size: 32,
        align: 8,
        members: &[(0, 8), (8, 8), (16, 8), (24, 8)],
    },
];

const TYPES: &str = "\
typedef struct { unsigned int m0; } R4;
typedef struct { unsigned long long m0; } R8;
typedef struct { unsigned int m0, m1, m2; } R12;
typedef struct { unsigned long long m0, m1; } R16;
typedef struct { unsigned __int128 q; } R16A;
typedef struct { unsigned long long m0, m1, m2; } R24;
typedef struct { unsigned long long m0, m1, m2, m3; } R32;
";

// ---------------------------------------------------------------------------
// EvoIR by hand: the front end does not emit the marks yet
// ---------------------------------------------------------------------------

/// A running SSA numbering and the memory and world tokens threaded with it.
struct Body {
    text: String,
    next: u32,
    mem: u32,
    world: u32,
}

impl Body {
    fn fresh(&mut self) -> u32 {
        let id = self.next;
        self.next += 1;
        id
    }

    fn line(&mut self, text: &str) {
        self.text.push_str("    ");
        self.text.push_str(text);
        self.text.push('\n');
    }

    fn constant(&mut self, width: u16, value: u64) -> u32 {
        let id = self.fresh();
        self.line(&format!("%v{id} = const.bits<{width}> {value}"));
        id
    }

    /// `base + offset`, or `base` itself at offset zero.
    fn at(&mut self, base: u32, offset: u64) -> u32 {
        if offset == 0 {
            return base;
        }
        let index = self.constant(64, offset);
        let id = self.fresh();
        self.line(&format!(
            "%v{id} = ptr.offset.add.unsigned %v{base}, %v{index} [scale=1]"
        ));
        let checked = self.fresh();
        self.line(&format!(
            "%v{checked} = pointer_valid %v{}, %v{id}",
            self.mem
        ));
        self.mem = checked;
        id
    }

    /// One member, zero-extended to `bits<64>` when it is narrower.
    fn load_member(&mut self, base: u32, offset: u64, width: u64) -> u32 {
        let pointer = self.at(base, offset);
        let checked = self.fresh();
        self.line(&format!(
            "%v{checked} = access_in_bounds %v{}, %v{pointer} [size={width}, align={width}]",
            self.mem
        ));
        let value = self.fresh();
        let mem = self.fresh();
        let world = self.fresh();
        self.line(&format!(
            "(%v{value}, %v{mem}, %v{world}) = load %v{checked}, %v{}, %v{pointer} : bits<{}>",
            self.world,
            width * 8
        ));
        self.mem = mem;
        self.world = world;
        if width == 4 {
            let wide = self.fresh();
            self.line(&format!("%v{wide} = zext.bits<64> %v{value}"));
            wide
        } else {
            value
        }
    }

    fn store_member(&mut self, base: u32, offset: u64, width: u64, value: u32) {
        let pointer = self.at(base, offset);
        let checked = self.fresh();
        self.line(&format!(
            "%v{checked} = access_in_bounds %v{}, %v{pointer} [size={width}, align={width}]",
            self.mem
        ));
        let mem = self.fresh();
        let world = self.fresh();
        self.line(&format!(
            "(%v{mem}, %v{world}) = store %v{checked}, %v{}, %v{pointer}, %v{value}",
            self.world
        ));
        self.mem = mem;
        self.world = world;
    }

    /// The checksum of a record at `base`: every member as `bits<64>`, summed.
    fn checksum(&mut self, record: &Record, base: u32) -> u32 {
        let mut total = None;
        for (offset, width) in record.members {
            let member = self.load_member(base, *offset, *width);
            total = Some(match total {
                None => member,
                Some(sum) => {
                    let next = self.fresh();
                    self.line(&format!("%v{next} = add.bits<64> %v{sum}, %v{member}"));
                    next
                }
            });
        }
        total.expect("a record has at least one member")
    }

    /// Member `i` set to `seed + i`, at each member's own width.
    fn fill(&mut self, record: &Record, base: u32, seed: u32) {
        for (index, (offset, width)) in record.members.iter().enumerate() {
            let step = self.constant(64, index as u64);
            let wide = self.fresh();
            self.line(&format!("%v{wide} = add.bits<64> %v{seed}, %v{step}"));
            let value = if *width == 4 {
                let narrow = self.fresh();
                self.line(&format!("%v{narrow} = trunc.bits<32> %v{wide}"));
                narrow
            } else {
                wide
            };
            self.store_member(base, *offset, *width, value);
        }
    }
}

/// Section 4.6's exact summary for the three shapes of body below.
const READS: &str = "effects { reads={all}, writes={}, volatile=false, atomic=false, io=false, may_trap=true, may_unwind=false }";
const WRITES: &str = "effects { reads={all}, writes={all}, volatile=false, atomic=false, io=false, may_trap=true, may_unwind=false }";
const CALLS: &str = "effects { reads={all}, writes={all}, volatile=true, atomic=true, io=true, may_trap=true, may_unwind=true }";

fn header(name: &str, params: &[String], results: &[&str], effects: &str) -> String {
    let mut spelled: Vec<String> = params
        .iter()
        .enumerate()
        .map(|(index, ty)| format!("%arg{index}: {ty}"))
        .collect();
    spelled.push(format!("%arg{}: mem<all>", params.len()));
    spelled.push(format!("%arg{}: world", params.len() + 1));
    let mut returns: Vec<&str> = results.to_vec();
    returns.push("mem<all>");
    returns.push("world");
    format!(
        "func @{name}({}) -> ({}) cc = target_default\n    {effects}\n{{\n",
        spelled.join(", "),
        returns.join(", ")
    )
}

/// The entry block's argument list, and a `Body` numbered past it.
fn open(params: &[&str]) -> (String, Body, Vec<u32>) {
    let mut entry = String::from("^entry(");
    let mut ids = Vec::new();
    for (index, ty) in params.iter().enumerate() {
        if index != 0 {
            entry.push_str(", ");
        }
        entry.push_str(&format!("%v{index}: {ty}"));
        ids.push(index as u32);
    }
    let mem = params.len() as u32;
    let world = mem + 1;
    entry.push_str(&format!("%v{mem}: mem<all>, %v{world}: world):\n"));
    if !params.is_empty() {
        entry = entry.replacen(&format!("%v{mem}"), &format!(", %v{mem}"), 1);
    }
    let body = Body {
        text: String::new(),
        next: world + 1,
        mem,
        world,
    };
    (entry, body, ids)
}

fn close(mut header: String, entry: String, body: Body, results: &[u32]) -> String {
    header.push_str(&entry);
    header.push_str(&body.text);
    let mut values: Vec<String> = results.iter().map(|id| format!("%v{id}")).collect();
    values.push(format!("%v{}", body.mem));
    values.push(format!("%v{}", body.world));
    header.push_str(&format!("    ret {}\n}}\n", values.join(", ")));
    header
}

fn byval(record: &Record) -> String {
    format!("ptr<generic> byval({}, {})", record.size, record.align)
}

fn sret(record: &Record) -> String {
    format!("ptr<generic> sret({}, {})", record.size, record.align)
}

/// `unsigned long long evo_take_R(R v)`.
fn take(record: &Record) -> String {
    let head = header(
        &format!("evo_take_{}", record.tag),
        &[byval(record)],
        &["bits<64>"],
        READS,
    );
    let (entry, mut body, ids) = open(&["ptr<generic>"]);
    let sum = body.checksum(record, ids[0]);
    close(head, entry, body, &[sum])
}

/// `unsigned long long evo_take_after_n_R(unsigned long long..., R v)`.
fn take_after(record: &Record, scalars: usize) -> String {
    let mut params: Vec<String> = (0..scalars).map(|_| "bits<64>".to_owned()).collect();
    params.push(byval(record));
    let head = header(
        &format!("evo_take_after_{scalars}_{}", record.tag),
        &params,
        &["bits<64>"],
        READS,
    );
    let types: Vec<&str> = params
        .iter()
        .map(|ty| {
            if ty == "bits<64>" {
                "bits<64>"
            } else {
                "ptr<generic>"
            }
        })
        .collect();
    let (entry, mut body, ids) = open(&types);
    let mut total = body.checksum(record, ids[scalars]);
    for id in &ids[..scalars] {
        let next = body.fresh();
        body.line(&format!("%v{next} = add.bits<64> %v{total}, %v{id}"));
        total = next;
    }
    close(head, entry, body, &[total])
}

/// `R evo_make_R(unsigned long long x)`.
fn make(record: &Record) -> String {
    let head = header(
        &format!("evo_make_{}", record.tag),
        &["bits<64>".to_owned(), sret(record)],
        &[],
        WRITES,
    );
    let (entry, mut body, ids) = open(&["bits<64>", "ptr<generic>"]);
    body.fill(record, ids[1], ids[0]);
    close(head, entry, body, &[])
}

/// `R evo_round_R(R v)`: every member one greater, so the result is the record
/// the host's own `fill(x + 1)` builds.
fn round(record: &Record) -> String {
    let head = header(
        &format!("evo_round_{}", record.tag),
        &[byval(record), sret(record)],
        &[],
        WRITES,
    );
    let (entry, mut body, ids) = open(&["ptr<generic>", "ptr<generic>"]);
    for (offset, width) in record.members {
        let member = body.load_member(ids[0], *offset, *width);
        let one = body.constant(64, 1);
        let sum = body.fresh();
        body.line(&format!("%v{sum} = add.bits<64> %v{member}, %v{one}"));
        let value = if *width == 4 {
            let narrow = body.fresh();
            body.line(&format!("%v{narrow} = trunc.bits<32> %v{sum}"));
            narrow
        } else {
            sum
        };
        body.store_member(ids[1], *offset, *width, value);
    }
    close(head, entry, body, &[])
}

/// `unsigned long long evo_forward_R(unsigned long long..., R v)`: copies the
/// record as EvoIR 10.1.3 requires and hands the copy to the host.
fn forward(record: &Record, scalars: usize, origin: u64) -> String {
    let mut params: Vec<String> = (0..scalars).map(|_| "bits<64>".to_owned()).collect();
    params.push(byval(record));
    let head = header(
        &format!("evo_forward_{scalars}_{}", record.tag),
        &params,
        &["bits<64>"],
        CALLS,
    );
    let mut types: Vec<&str> = (0..scalars).map(|_| "bits<64>").collect();
    types.push("ptr<generic>");
    let (entry, mut body, ids) = open(&types);
    let copy = body.fresh();
    let mem = body.fresh();
    let world = body.fresh();
    body.line(&format!(
        "(%v{copy}, %v{mem}, %v{world}) = alloca %v{}, %v{} [origin={origin}, size={}, align={}]",
        body.mem, body.world, record.size, record.align
    ));
    body.mem = mem;
    body.world = world;
    let mem = body.fresh();
    let world = body.fresh();
    body.line(&format!(
        "(%v{mem}, %v{world}) = copy %v{}, %v{}, %v{copy}, %v{} [size={}, align={}]",
        body.mem, body.world, ids[scalars], record.size, record.align
    ));
    body.mem = mem;
    body.world = world;
    let result = body.fresh();
    let mem = body.fresh();
    let world = body.fresh();
    let mut arguments: Vec<String> = ids[..scalars].iter().map(|id| format!("%v{id}")).collect();
    arguments.push(format!("%v{copy}"));
    arguments.push(format!("%v{}", body.mem));
    arguments.push(format!("%v{}", body.world));
    body.line(&format!(
        "(%v{result}, %v{mem}, %v{world}) = call @host_take_{}_{}({})",
        scalars,
        record.tag,
        arguments.join(", ")
    ));
    body.mem = mem;
    body.world = world;
    close(head, entry, body, &[result])
}

/// `unsigned long long evo_want_R(unsigned long long x)`: asks the host for a
/// record and checksums what it wrote into the destination.
fn want(record: &Record, origin: u64) -> String {
    let head = header(
        &format!("evo_want_{}", record.tag),
        &["bits<64>".to_owned()],
        &["bits<64>"],
        CALLS,
    );
    let (entry, mut body, ids) = open(&["bits<64>"]);
    let destination = body.fresh();
    let mem = body.fresh();
    let world = body.fresh();
    body.line(&format!(
        "(%v{destination}, %v{mem}, %v{world}) = alloca %v{}, %v{} [origin={origin}, size={}, align={}]",
        body.mem, body.world, record.size, record.align
    ));
    body.mem = mem;
    body.world = world;
    let mem = body.fresh();
    let world = body.fresh();
    body.line(&format!(
        "(%v{mem}, %v{world}) = call @host_make_{}(%v{}, %v{destination}, %v{}, %v{})",
        record.tag, ids[0], body.mem, body.world
    ));
    body.mem = mem;
    body.world = world;
    let sum = body.checksum(record, destination);
    close(head, entry, body, &[sum])
}

/// `unsigned long long evo_indirect_R(R v)`: the same forwarding call, made
/// through a function pointer whose own type carries the marks.
fn indirect(record: &Record, origin: u64) -> String {
    let head = header(
        &format!("evo_indirect_{}", record.tag),
        &[byval(record)],
        &["bits<64>"],
        CALLS,
    );
    let (entry, mut body, ids) = open(&["ptr<generic>"]);
    let copy = body.fresh();
    let mem = body.fresh();
    let world = body.fresh();
    body.line(&format!(
        "(%v{copy}, %v{mem}, %v{world}) = alloca %v{}, %v{} [origin={origin}, size={}, align={}]",
        body.mem, body.world, record.size, record.align
    ));
    body.mem = mem;
    body.world = world;
    let mem = body.fresh();
    let world = body.fresh();
    body.line(&format!(
        "(%v{mem}, %v{world}) = copy %v{}, %v{}, %v{copy}, %v{} [size={}, align={}]",
        body.mem, body.world, ids[0], record.size, record.align
    ));
    body.mem = mem;
    body.world = world;
    let pointer = body.fresh();
    body.line(&format!(
        "%v{pointer} = func_addr @host_take_0_{} : fnptr<({}, mem<all>, world)->(bits<64>, mem<all>, world)>",
        record.tag,
        byval(record)
    ));
    let result = body.fresh();
    let mem = body.fresh();
    let world = body.fresh();
    body.line(&format!(
        "(%v{result}, %v{mem}, %v{world}) = call %v{pointer}(%v{copy}, %v{}, %v{})",
        body.mem, body.world
    ));
    body.mem = mem;
    body.world = world;
    close(head, entry, body, &[result])
}

/// `unsigned long long evo_indirect_want_R(unsigned long long x)`: a record
/// result asked for through a function pointer.
fn indirect_want(record: &Record, origin: u64) -> String {
    let head = header(
        &format!("evo_indirect_want_{}", record.tag),
        &["bits<64>".to_owned()],
        &["bits<64>"],
        CALLS,
    );
    let (entry, mut body, ids) = open(&["bits<64>"]);
    let destination = body.fresh();
    let mem = body.fresh();
    let world = body.fresh();
    body.line(&format!(
        "(%v{destination}, %v{mem}, %v{world}) = alloca %v{}, %v{} [origin={origin}, size={}, align={}]",
        body.mem, body.world, record.size, record.align
    ));
    body.mem = mem;
    body.world = world;
    let pointer = body.fresh();
    body.line(&format!(
        "%v{pointer} = func_addr @host_make_{} : fnptr<(bits<64>, {}, mem<all>, world)->(mem<all>, world)>",
        record.tag,
        sret(record)
    ));
    let mem = body.fresh();
    let world = body.fresh();
    body.line(&format!(
        "(%v{mem}, %v{world}) = call %v{pointer}(%v{}, %v{destination}, %v{}, %v{})",
        ids[0], body.mem, body.world
    ));
    body.mem = mem;
    body.world = world;
    let sum = body.checksum(record, destination);
    close(head, entry, body, &[sum])
}

/// The `declare`s of the host side, with the marks the machine reads.
fn declares(positions: &[usize]) -> String {
    let mut text = String::new();
    // The indirect calls always name the first position, so it is declared
    // whether or not the driver exercises it.
    let mut positions: Vec<usize> = positions.to_vec();
    if !positions.contains(&0) {
        positions.insert(0, 0);
    }
    for record in RECORDS {
        for scalars in &positions {
            let mut params: Vec<String> = (0..*scalars).map(|_| "bits<64>".to_owned()).collect();
            params.push(byval(record));
            params.push("mem<all>".to_owned());
            params.push("world".to_owned());
            text.push_str(&format!(
                "declare @host_take_{scalars}_{}({}) -> (bits<64>, mem<all>, world) cc = target_default\n",
                record.tag,
                params.join(", ")
            ));
        }
        text.push_str(&format!(
            "declare @host_make_{}(bits<64>, {}, mem<all>, world) -> (mem<all>, world) cc = target_default\n",
            record.tag,
            sret(record)
        ));
    }
    text
}

/// The whole hand-written module: every record, in every position `positions`
/// names, on both sides of the call.
fn module_text(positions: &[usize]) -> String {
    let mut functions = Vec::new();
    // Every `alloca` origin in a module is its own, as Section 10.1.3 requires
    // of the temporaries a record call needs.
    let mut origin = 0_u64;
    for record in RECORDS {
        functions.push(take(record));
        functions.push(make(record));
        functions.push(round(record));
        functions.push(want(record, origin));
        origin += 1;
        functions.push(indirect(record, origin));
        origin += 1;
        functions.push(indirect_want(record, origin));
        origin += 1;
        for scalars in positions {
            if *scalars > 0 {
                functions.push(take_after(record, *scalars));
            }
            functions.push(forward(record, *scalars, origin));
            origin += 1;
        }
    }
    format!("{}\n{}", declares(positions), functions.join("\n"))
}

// ---------------------------------------------------------------------------
// The host driver
// ---------------------------------------------------------------------------

fn c_sum(record: &Record) -> String {
    if record.tag == "R16A" {
        return format!(
            "static unsigned long long sum_{0}({0} v) {{ return (unsigned long long)v.q + (unsigned long long)(v.q >> 64); }}\n",
            record.tag
        );
    }
    let terms: Vec<String> = (0..record.members.len())
        .map(|index| format!("(unsigned long long)v.m{index}"))
        .collect();
    format!(
        "static unsigned long long sum_{0}({0} v) {{ return {1}; }}\n",
        record.tag,
        terms.join(" + ")
    )
}

fn c_fill(record: &Record) -> String {
    if record.tag == "R16A" {
        return format!(
            "static {0} fill_{0}(unsigned long long x) {{ {0} v; v.q = ((unsigned __int128)(x + 1) << 64) | (unsigned __int128)x; return v; }}\n",
            record.tag
        );
    }
    let assignments: Vec<String> = record
        .members
        .iter()
        .enumerate()
        .map(|(index, (_, width))| {
            let cast = if *width == 4 {
                "(unsigned int)"
            } else {
                "(unsigned long long)"
            };
            format!("v.m{index} = {cast}(x + {index});")
        })
        .collect();
    format!(
        "static {0} fill_{0}(unsigned long long x) {{ {0} v; {1} return v; }}\n",
        record.tag,
        assignments.join(" ")
    )
}

fn driver(positions: &[usize]) -> String {
    let mut text = String::from("#include <stdio.h>\n#include <string.h>\n\n");
    text.push_str(TYPES);
    text.push('\n');
    for record in RECORDS {
        text.push_str(&c_sum(record));
        text.push_str(&c_fill(record));
    }
    text.push('\n');
    // The host side of every call EvoCC makes.
    for record in RECORDS {
        for scalars in positions {
            let mut params: Vec<String> = (0..*scalars)
                .map(|index| format!("unsigned long long a{index}"))
                .collect();
            params.push(format!("{} v", record.tag));
            let mut sum = format!("sum_{}(v)", record.tag);
            for index in 0..*scalars {
                sum = format!("{sum} + a{index}");
            }
            text.push_str(&format!(
                "unsigned long long host_take_{scalars}_{}({}) {{ return {sum}; }}\n",
                record.tag,
                params.join(", ")
            ));
        }
        text.push_str(&format!(
            "{0} host_make_{0}(unsigned long long x) {{ return fill_{0}(x); }}\n",
            record.tag
        ));
    }
    text.push('\n');
    // What EvoCC compiled.
    for record in RECORDS {
        text.push_str(&format!(
            "unsigned long long evo_take_{0}({0});\n{0} evo_make_{0}(unsigned long long);\n{0} evo_round_{0}({0});\nunsigned long long evo_want_{0}(unsigned long long);\nunsigned long long evo_indirect_{0}({0});\nunsigned long long evo_indirect_want_{0}(unsigned long long);\n",
            record.tag
        ));
        for scalars in positions {
            let mut params: Vec<String> = (0..*scalars)
                .map(|_| "unsigned long long".to_owned())
                .collect();
            params.push(record.tag.to_owned());
            if *scalars > 0 {
                text.push_str(&format!(
                    "unsigned long long evo_take_after_{scalars}_{}({});\n",
                    record.tag,
                    params.join(", ")
                ));
            }
            text.push_str(&format!(
                "unsigned long long evo_forward_{scalars}_{}({});\n",
                record.tag,
                params.join(", ")
            ));
        }
    }
    text.push_str(
        "\nstatic int failures = 0;\nstatic int checks = 0;\nstatic void expect(const char *what, unsigned long long got, unsigned long long want) {\n    checks++;\n    if (got != want) { failures++; printf(\"MISMATCH %s got %llu want %llu\\n\", what, got, want); }\n}\n\nint main(void) {\n    static const unsigned long long seeds[] = { 1ULL, 3ULL, 12345ULL, 0x0f0f0f0fULL };\n    for (unsigned i = 0; i < sizeof seeds / sizeof seeds[0]; i++) {\n        unsigned long long x = seeds[i];\n",
    );
    for record in RECORDS {
        let tag = record.tag;
        text.push_str(&format!(
            "        {{\n            {tag} v = fill_{tag}(x);\n            expect(\"take {tag}\", evo_take_{tag}(v), sum_{tag}(v));\n            {tag} made = evo_make_{tag}(x);\n            expect(\"make {tag}\", (unsigned long long)(memcmp(&made, &v, sizeof v) == 0), 1ULL);\n            {tag} next = fill_{tag}(x + 1);\n            {tag} rounded = evo_round_{tag}(v);\n            expect(\"round {tag}\", (unsigned long long)(memcmp(&rounded, &next, sizeof next) == 0), 1ULL);\n            expect(\"want {tag}\", evo_want_{tag}(x), sum_{tag}(v));\n            expect(\"indirect {tag}\", evo_indirect_{tag}(v), sum_{tag}(v));\n            expect(\"indirect want {tag}\", evo_indirect_want_{tag}(x), sum_{tag}(v));\n"
        ));
        for scalars in positions {
            let args: Vec<String> = (0..*scalars)
                .map(|index| format!("{}ULL", index + 100))
                .collect();
            let extra: u64 = (0..*scalars as u64).map(|index| index + 100).sum();
            let call_args = if args.is_empty() {
                "v".to_owned()
            } else {
                format!("{}, v", args.join(", "))
            };
            if *scalars > 0 {
                text.push_str(&format!(
                    "            expect(\"take_after_{scalars} {tag}\", evo_take_after_{scalars}_{tag}({call_args}), sum_{tag}(v) + {extra}ULL);\n"
                ));
            }
            text.push_str(&format!(
                "            expect(\"forward_{scalars} {tag}\", evo_forward_{scalars}_{tag}({call_args}), sum_{tag}(v) + {extra}ULL);\n"
            ));
        }
        text.push_str("        }\n");
    }
    text.push_str(
        "    }\n    printf(\"checks=%d failures=%d\\n\", checks, failures);\n    return failures == 0 ? 0 : 1;\n}\n",
    );
    text
}

// ---------------------------------------------------------------------------
// Darwin
// ---------------------------------------------------------------------------

/// The positions both targets admit: a record first, after one scalar and after
/// two, which is where the even-register rounding shows.
const DARWIN_POSITIONS: &[usize] = &[0, 1, 2];
/// Linux also lowers stack arguments, so the record after seven scalars is
/// there too: the whole record goes to the stack and `x7` is unused.
const LINUX_POSITIONS: &[usize] = &[0, 1, 2, 7];

#[test]
fn records_cross_the_boundary_on_darwin() {
    let toolchain = match Toolchain::locate() {
        Ok(toolchain) => toolchain,
        Err(reason) => {
            println!("skipped={reason}");
            return;
        }
    };
    println!("toolchain={}", toolchain.version());

    let text = module_text(DARWIN_POSITIONS);
    let module = parse_section9_module(text.as_bytes()).expect("canonical EvoIR");
    evocc_evoir::validate_module(&module).expect("the record module validates");
    let assembly = compile_module_with(&module, &|_| true)
        .expect("every record function lowers, allocates, verifies and emits");

    let work = std::env::temp_dir().join(format!("evocc-records-darwin-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).expect("a working directory");
    std::fs::write(work.join("records.s"), &assembly).expect("the emitted text");
    std::fs::write(work.join("driver.c"), driver(DARWIN_POSITIONS)).expect("the host driver");

    let built = Command::new("clang")
        .arg("-O2")
        .arg("-o")
        .arg(work.join("records"))
        .arg(work.join("driver.c"))
        .arg(work.join("records.s"))
        .output()
        .expect("run clang");
    assert!(
        built.status.success(),
        "clang refused the record probe:\n{}",
        String::from_utf8_lossy(&built.stderr)
    );
    let run = Command::new(work.join("records"))
        .output()
        .expect("run the record probe");
    let stdout = String::from_utf8_lossy(&run.stdout).into_owned();
    print!("{stdout}");
    assert!(
        stdout.contains("failures=0") && run.status.code() == Some(0),
        "the Darwin lane reported a disagreement:\n{stdout}"
    );
    let _ = std::fs::remove_dir_all(&work);
}

// ---------------------------------------------------------------------------
// Linux, in the sealed container
// ---------------------------------------------------------------------------

const CONTAINER_BIN: &str = "/usr/local/bin/container";
const IMAGE: &str = "evo-flight-lab:sealed";

fn container_available() -> bool {
    if !std::path::Path::new(CONTAINER_BIN).exists() {
        println!("skipped={CONTAINER_BIN} is absent");
        return false;
    }
    let listed = match Command::new(CONTAINER_BIN).args(["image", "list"]).output() {
        Ok(listed) if listed.status.success() => listed,
        _ => {
            println!("skipped=the container runtime is not started");
            return false;
        }
    };
    if !String::from_utf8_lossy(&listed.stdout).contains("evo-flight-lab") {
        println!("skipped={IMAGE} is not present");
        return false;
    }
    true
}

/// Build and run one program of `sources` in the sealed container.
fn run_in_container(work: &std::path::Path, program: &str) -> String {
    std::fs::write(
        work.join("runner.sh"),
        format!("#!/bin/sh\ncd /work || exit 2\ngcc-14 -O2 -o /work/{program} /work/driver.c /work/{program}.s 2>/work/build.err || {{ echo \"build $(head -c 600 /work/build.err | tr '\\n' ' ')\"; exit 0; }}\n/work/{program}\necho \"status $?\"\n"),
    )
    .expect("the runner");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mut permissions = std::fs::metadata(work.join("runner.sh"))
            .expect("the runner exists")
            .permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(work.join("runner.sh"), permissions)
            .expect("the runner is runnable");
    }
    let output = Command::new(CONTAINER_BIN)
        .arg("run")
        .arg("--rm")
        .arg("--platform")
        .arg("linux/arm64")
        .arg("--network")
        .arg("none")
        .arg("--volume")
        .arg(format!("{}:/work", work.display()))
        .arg(IMAGE)
        .arg("/work/runner.sh")
        .output()
        .expect("run the sealed container");
    String::from_utf8_lossy(&output.stdout).into_owned()
}

#[test]
fn records_cross_the_boundary_under_gcc14_in_the_sealed_container() {
    if !container_available() {
        return;
    }
    let text = module_text(LINUX_POSITIONS);
    let module = parse_section9_module(text.as_bytes()).expect("canonical EvoIR");
    evocc_evoir::validate_module(&module).expect("the record module validates");
    let assembly = compile_module_for(Target::Linux, &module, &|_| true)
        .expect("every record function lowers for Linux");

    let work = std::env::temp_dir().join(format!("evocc-records-linux-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).expect("a working directory");
    std::fs::write(work.join("records.s"), &assembly).expect("the emitted text");
    std::fs::write(work.join("driver.c"), driver(LINUX_POSITIONS)).expect("the host driver");

    let stdout = run_in_container(&work, "records");
    print!("{stdout}");
    assert!(
        !stdout.contains("build "),
        "GCC-14 refused the record probe:\n{stdout}"
    );
    assert!(
        stdout.contains("failures=0") && stdout.contains("status 0"),
        "the Linux lane reported a disagreement:\n{stdout}"
    );
    let _ = std::fs::remove_dir_all(&work);
}

// ---------------------------------------------------------------------------
// The zstd rung's reproducing function
// ---------------------------------------------------------------------------

/// `typedef struct { unsigned long long t; } T; T get(void); unsigned long long
/// span(T a, T b);` — the shape whose hidden pointer faulted inside a GCC-built
/// program (`tests/contracts/ladder-zstd-receipt.json`, "aggregate ABI").
const ZSTD: &str = "\
declare @get(ptr<generic> sret(8, 8), mem<all>, world) -> (mem<all>, world) cc = target_default

func @span(%arg0: ptr<generic> byval(8, 8), %arg1: ptr<generic> byval(8, 8), %arg2: mem<all>, %arg3: world) -> (bits<64>, mem<all>, world) cc = target_default
    effects { reads={all}, writes={}, volatile=false, atomic=false, io=false, may_trap=true, may_unwind=false }
{
^entry(%v0: ptr<generic>, %v1: ptr<generic>, %v2: mem<all>, %v3: world):
    %v4 = access_in_bounds %v2, %v0 [size=8, align=8]
    (%v5, %v6, %v7) = load %v4, %v3, %v0 : bits<64>
    %v8 = access_in_bounds %v6, %v1 [size=8, align=8]
    (%v9, %v10, %v11) = load %v8, %v7, %v1 : bits<64>
    %v12 = sub.bits<64> %v9, %v5
    ret %v12, %v10, %v11
}

func @use_get(%arg0: mem<all>, %arg1: world) -> (bits<64>, mem<all>, world) cc = target_default
    effects { reads={all}, writes={all}, volatile=true, atomic=true, io=true, may_trap=true, may_unwind=true }
{
^entry(%v0: mem<all>, %v1: world):
    (%v2, %v3, %v4) = alloca %v0, %v1 [origin=0, size=8, align=8]
    (%v5, %v6) = call @get(%v2, %v3, %v4)
    %v7 = access_in_bounds %v5, %v2 [size=8, align=8]
    (%v8, %v9, %v10) = load %v7, %v6, %v2 : bits<64>
    ret %v8, %v9, %v10
}
";

const ZSTD_DRIVER: &str = r#"#include <stdio.h>
typedef struct { unsigned long long t; } T;
unsigned long long span(T a, T b);
unsigned long long use_get(void);
T get(void) { T r; r.t = 0x0123456789abcdefULL; return r; }

int main(void) {
    int failures = 0;
    T a, b;
    a.t = 1000; b.t = 1729;
    if (span(a, b) != 729ULL) { failures++; printf("MISMATCH span %llu\n", span(a, b)); }
    if (use_get() != 0x0123456789abcdefULL) { failures++; printf("MISMATCH use_get %llu\n", use_get()); }
    printf("checks=2 failures=%d\n", failures);
    return failures == 0 ? 0 : 1;
}
"#;

#[test]
fn the_zstd_receipts_reproducing_function_links_and_runs_under_gcc14() {
    if !container_available() {
        return;
    }
    let module = parse_section9_module(ZSTD.as_bytes()).expect("canonical EvoIR");
    evocc_evoir::validate_module(&module).expect("the zstd shape validates");
    let assembly = compile_module_for(Target::Linux, &module, &|_| true)
        .expect("the zstd shape lowers for Linux");
    // The eight-byte record travels in a register on both sides, which is what
    // the receipt's defect was: a hidden pointer where AAPCS64 puts a register.
    assert!(
        !assembly.contains("x8,"),
        "an eight-byte record needs no indirect result:\n{assembly}"
    );

    let work = std::env::temp_dir().join(format!("evocc-zstd-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).expect("a working directory");
    std::fs::write(work.join("zstd.s"), &assembly).expect("the emitted text");
    std::fs::write(work.join("driver.c"), ZSTD_DRIVER).expect("the host driver");

    let stdout = run_in_container(&work, "zstd");
    print!("{stdout}");
    assert!(
        !stdout.contains("build "),
        "GCC-14 refused the zstd shape:\n{stdout}"
    );
    assert!(
        stdout.contains("failures=0") && stdout.contains("status 0"),
        "the zstd shape disagrees with GCC-14:\n{stdout}"
    );
    let _ = std::fs::remove_dir_all(&work);
}

// ---------------------------------------------------------------------------
// The register assignment itself
// ---------------------------------------------------------------------------

/// The emitted body of one function, without its prologue and epilogue.
fn body_of(assembly: &str, name: &str) -> String {
    let start = assembly
        .find(&format!("\n{name}:"))
        .or_else(|| assembly.find(&format!("\n_{name}:")))
        .unwrap_or_else(|| panic!("@{name} is absent from the emitted text"));
    let rest = &assembly[start + 1..];
    let end = rest.find("\n\n").unwrap_or(rest.len());
    rest[..end].to_owned()
}

fn emitted(target: Target, positions: &[usize]) -> String {
    let text = module_text(positions);
    let module = parse_section9_module(text.as_bytes()).expect("canonical EvoIR");
    evocc_evoir::validate_module(&module).expect("the record module validates");
    compile_module_for(target, &module, &|_| true).expect("every record function lowers")
}

#[test]
fn zero_sized_aligned_arguments_do_not_skip_a_register() {
    let text = "func @after(%arg0: bits<64>, %arg1: ptr<generic> byval(0, 16), %arg2: bits<64>) -> (bits<64>) cc = target_default\n    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, may_trap=false, may_unwind=false }\n{\n^entry(%v0: bits<64>, %v1: ptr<generic>, %v2: bits<64>):\n    ret %v2\n}\n";
    let module = parse_section9_module(text.as_bytes()).unwrap();
    for target in [Target::Linux, Target::Darwin] {
        let assembly = compile_module_for(target, &module, &|_| true).unwrap();
        assert!(
            body_of(&assembly, "after").contains("mov x0, x1"),
            "{target:?}: {assembly}"
        );
    }
}

#[test]
fn a_record_of_at_most_sixteen_bytes_arrives_in_registers() {
    for target in [Target::Darwin, Target::Linux] {
        let assembly = emitted(target, &[0]);
        // Section 6: "in the callee, stored into a frame slot that becomes the
        // parameter's place".
        let take8 = body_of(&assembly, "evo_take_R8");
        assert!(
            take8.contains("str x0, [x1, #0]"),
            "an eight-byte record arrives in x0 ({target:?}):\n{take8}"
        );
        let take16 = body_of(&assembly, "evo_take_R16");
        assert!(
            take16.contains("str x0, ") && take16.contains("str x1, "),
            "a sixteen-byte record arrives in x0 and x1 ({target:?}):\n{take16}"
        );
        // Section 6: "a `byval` record above 16 bytes stays a pointer to the
        // caller's copy", so nothing is stored into a slot of its own.
        let take24 = body_of(&assembly, "evo_take_R24");
        assert!(
            take24.starts_with(&format!(
                "{}evo_take_R24:",
                if matches!(target, Target::Darwin) {
                    "_"
                } else {
                    ""
                }
            )) && take24.contains("ldr x1, [x0, #0]"),
            "a twenty-four-byte record arrives as a pointer in x0 ({target:?}):\n{take24}"
        );
    }
}

#[test]
fn an_alignment_of_sixteen_starts_at_an_even_register_on_linux_only() {
    // Section 6: "a record whose alignment is 16 starts at an even register on
    // Linux, as a `bits<128>` does". Apple does not round: measured on both
    // oracles in `.evidence/builtin-probes/20260906-b10/`, functions
    // `after_one_16a16`.
    let linux = body_of(&emitted(Target::Linux, &[1]), "evo_take_after_1_R16A");
    assert!(
        linux.contains("mov x1, x2") && linux.contains("mov x2, x3"),
        "Linux skips x1 and takes x2:x3:\n{linux}"
    );
    let darwin = body_of(&emitted(Target::Darwin, &[1]), "evo_take_after_1_R16A");
    assert!(
        darwin.contains("str x1, ")
            && darwin.contains("str x2, ")
            && !darwin.contains("mov x2, x3"),
        "Darwin takes x1:x2 with no rounding:\n{darwin}"
    );
    // An alignment of eight rounds on neither target.
    for target in [Target::Darwin, Target::Linux] {
        let plain = body_of(&emitted(target, &[1]), "evo_take_after_1_R16");
        assert!(
            plain.contains("str x1, ") && plain.contains("str x2, "),
            "an eight-aligned record after one scalar is x1:x2 ({target:?}):\n{plain}"
        );
    }
}

#[test]
fn a_record_result_above_sixteen_bytes_travels_in_x8() {
    for target in [Target::Darwin, Target::Linux] {
        let assembly = emitted(target, &[0]);
        // Section 6: "above 16 bytes the destination pointer travels in `x8`,
        // not in an argument register, and the callee writes through `x8`".
        let make24 = body_of(&assembly, "evo_make_R24");
        assert!(
            make24.contains("mov x1, x8"),
            "the callee reads its destination out of x8 ({target:?}):\n{make24}"
        );
        let want24 = body_of(&assembly, "evo_want_R24");
        let call = want24
            .find("bl ")
            .unwrap_or_else(|| panic!("@evo_want_R24 calls the host:\n{want24}"));
        assert!(
            want24[..call].contains("mov x8, "),
            "the caller sets x8 before the call ({target:?}):\n{want24}"
        );
        // At most sixteen bytes there is no indirect result at all.
        let make16 = body_of(&assembly, "evo_make_R16");
        assert!(
            !make16.contains("x8"),
            "a sixteen-byte result comes back in x0 and x1 ({target:?}):\n{make16}"
        );
    }
}

#[test]
fn darwin_refuses_a_record_that_would_need_the_stack() {
    // Section 10: `E4010` is "more than eight integer arguments on Darwin". A
    // sixteen-byte record after seven scalars needs nine positions, because
    // AAPCS64 sends the whole record to the stack rather than splitting it.
    let text = module_text(&[7]);
    let module = parse_section9_module(text.as_bytes()).expect("canonical EvoIR");
    let refusals = evocc_mir::lower_module(&module)
        .expect_err("Darwin has no measured rule for a stack argument");
    let mut codes: Vec<&str> = refusals.iter().map(|refusal| refusal.code).collect();
    codes.sort_unstable();
    codes.dedup();
    assert_eq!(codes, vec!["E4010"], "{refusals:?}");
}

#[test]
fn e4011_is_withdrawn() {
    // Section 10: "`E4011` ... withdrawn at revision 0.18: `byval`/`sret`
    // records follow AAPCS64 (Section 6)". Every record shape lowers, so no
    // aggregate refusal is left to report.
    let text = module_text(&[0, 1, 2]);
    let module = parse_section9_module(text.as_bytes()).expect("canonical EvoIR");
    for target in [Target::Darwin, Target::Linux] {
        let lowered =
            evocc_mir::lower_module_for(target, &module, &|name| name.starts_with("host_"));
        assert!(
            lowered.is_ok(),
            "records are lowered, not refused ({target:?}): {:?}",
            lowered.err()
        );
    }
    println!("aggregateRefusalsWithdrawn=E4011");
}
