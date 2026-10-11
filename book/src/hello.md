# Hello, World!

Now that you have installed EvoCC, it is time to write your first program. It is a tradition to
start with a program that prints `Hello, world!`, so we do the same here.

Make a directory for the programs in this book, and go into it:

```console
$ mkdir hello
$ cd hello
```

## Write and run the program

Make a file `hello.c` with this content:

```c
#include <stdio.h>

int main(void) {
    printf("Hello, world!\n");
    return 0;
}
```

Compile it and run it:

```console
$ evocc hello.c -o hello
$ ./hello
Hello, world!
```

If you have used `gcc` or `clang`, this looks the same, and that is the intent: `evocc` takes the
options of a usual C compiler.

## One step at a time

When you give `evocc` a C file, it does four things:

1. It *preprocesses* the file: it reads `#include` and `#define` lines and expands macros.
2. It *compiles* the result to AArch64 assembly. This is the part that EvoCC does itself.
3. It *assembles* the assembly into an object file, with the assembler of your system.
4. It *links* the object files into a program, with the linker of your system.

You can stop after each step:

```console
$ evocc -E hello.c -o hello.i
$ evocc -S hello.c -o hello.s
$ evocc -c hello.c -o hello.o
$ evocc hello.o -o hello
```

`-E` stops after the preprocessor, `-S` after the compiler and `-c` after the assembler. The last
command only links.

EvoCC makes code for the host by default: `aarch64-apple-darwin` on macOS and
`aarch64-unknown-linux-gnu` on Linux. `--target` selects the other one. The development options of
the next chapters, such as `--emit=asm` and the interpreters, use `aarch64-unknown-linux-gnu` when
you do not give `--target`.

`evo help`, `evo --help` and `evo VERB --help` print usage and exit 0.
With no arguments, `evo` prints the same general usage and exits 64.
Append `--json` to a help request for one JSON line with `usage` (string)
and `exit_code` (integer). Help does not start the compiler.

## Cross-compile for Linux

The Linux headers go in `sysroot/aarch64-unknown-linux-gnu/`, beside `bin/`.
With this layout, the compiler finds them without an include flag:

```console
$ evocc --target=aarch64-unknown-linux-gnu -c hello.c -o hello.o
$ file hello.o
hello.o: ELF 64-bit LSB relocatable, ARM aarch64, version 1 (SYSV), not stripped
```

The bundled sysroot does not support these eight netfilter headers. On a case-insensitive volume
an include of one of them resolves to the lowercase file of the same name; on a case-sensitive
volume it is not found. EvoCC makes no promise about either result. The contract field
[`excluded_case_collisions`](https://github.com/onehr/evocc/blob/main/contracts/sysroot-aarch64-unknown-linux-gnu.json) lists them.

The paths below are relative to `usr/include/`:
`linux/netfilter/xt_CONNMARK.h`, `linux/netfilter/xt_DSCP.h`, `linux/netfilter/xt_MARK.h`,
`linux/netfilter/xt_RATEEST.h`, `linux/netfilter/xt_TCPMSS.h`,
`linux/netfilter_ipv4/ipt_ECN.h`, `linux/netfilter_ipv4/ipt_TTL.h`, `linux/netfilter_ipv6/ip6t_HL.h`.

To use another root, give `--sysroot=DIR` or set `EVOCC_SYSROOT`. The explicit
option takes priority. `-v` prints the selected root.

These commands also need an AArch64 Linux assembler. On the tested macOS host,
the default assembler runs in the existing `evo-flight-lab:sealed` container.
Use `--assembler=COMMAND` to select another assembler. The header bundle
contains no linker, libc or runtime. Linux linking on a non-Linux host requires
an explicit `--linker=COMMAND`; without it, stop at `-c`.

## More than one file

Real programs have more than one file. Make a header `greet.h`:

```c
void greet(const char *name);
```

a file `greet.c`:

```c
#include <stdio.h>
#include "greet.h"

void greet(const char *name) {
    printf("Hello, %s!\n", name);
}
```

and a file `main.c`:

```c
#include "greet.h"

int main(void) {
    greet("EvoCC");
    return 0;
}
```

You can give both C files to `evocc` at once:

```console
$ evocc main.c greet.c -o greet
$ ./greet
Hello, EvoCC!
```

Or you can compile each file on its own, the way `make` does, and link at the end. `-MMD -MF`
writes a dependency file for `make`. It lists the headers that each file includes:

```console
$ evocc -c main.c -o main.o -MMD -MF main.d
$ evocc -c greet.c -o greet.o -MMD -MF greet.d
$ evocc main.o greet.o -o greet
$ ./greet
Hello, EvoCC!
$ cat main.d
main.o: main.c greet.h
```

Both ways make the same program.

## CMake

Use the toolchain file from an EvoCC source checkout. Use absolute paths for
the toolchain file and compiler. CMake uses its standard GNU-compatible
compiler identification; no compiler-ID module or wrapper is needed.
`__EVOCC__` is `1`, and `__EVOCC_VERSION__` is the compiler version string.
`__GNUC__` remains available for compatible headers.

For example, build cJSON 1.7.19 with its utility tests:

```sh
cmake -S /path/to/cJSON-1.7.19 -B build/cjson \
  -DCMAKE_TOOLCHAIN_FILE=/path/to/evocc/cmake/evocc.cmake \
  -DCMAKE_C_COMPILER=/path/to/evocc/bin/evocc \
  -DCMAKE_POLICY_VERSION_MINIMUM=3.5 -DENABLE_CJSON_UTILS=ON
cmake --build build/cjson --parallel 2
ctest --test-dir build/cjson --output-on-failure
```

The policy option lets CMake 4 use this older upstream project. It does not
change cJSON's sources. The default target is the host. Set `EVOCC_TARGET`
to choose `aarch64-apple-darwin` or `aarch64-unknown-linux-gnu` explicitly.

For a Linux object build on macOS, use a project with an `OBJECT` library:

```cmake
cmake_minimum_required(VERSION 3.16)
project(HelloObject LANGUAGES C)
add_library(hello OBJECT hello.c)
```

Configure and build with the bundled Linux headers:

```sh
export EVOCC_AS='clang --target=aarch64-linux-gnu'
cmake -S . -B build/linux \
  -DCMAKE_TOOLCHAIN_FILE=/path/to/evocc/cmake/evocc.cmake \
  -DCMAKE_C_COMPILER=/path/to/evocc/bin/evocc \
  -DEVOCC_TARGET=aarch64-unknown-linux-gnu \
  -DCMAKE_SYSROOT=/path/to/evocc/sysroot/aarch64-unknown-linux-gnu
cmake --build build/linux --parallel 2
```

Cross-target configuration compiles and archives its compiler probes. The
header bundle does not support an executable link by itself. A Linux link
still needs target libraries and an explicit linker. The toolchain preserves
user compilation flags and uses the supplied sysroot during compiler detection.

The default zlib 1.3.2 CMake build and all 16 CTest commands pass on this macOS
host. EvoCC does not produce coverage data. Apple's `gcov` reports the missing
`.gcno` file but exits 0, so the coverage-summary test does not prove coverage support.

## Run a program without compiling it

`evo run` runs `main` in the C-HIR interpreter, without assembly or a linker.
Its exit status is the program's return value. Make a file `sum.c`:

```c
static int sum_to(int n) {
    int total = 0;
    for (int i = 1; i <= n; i++)
        total += i;
    return total;
}

int main(void) {
    return sum_to(10);
}
```

Run it, then inspect the exit status:

```console
$ evo run sum.c; echo $?
55
```

Use `evo run FILE.c -- ARGS` to pass arguments to `main(int argc, char **argv)`.
The developer commands `evocc --interpret-c-hir` and `evocc --interpret-evoir`
print the return value instead. [Chapter 7](how-it-works.md) explains both IRs.

The interpreters run only the code in your file. They do not call into the C library. To see
this, make a file `shout.c` that calls `puts`. It declares `puts` by hand, so it needs no header:

```c
int puts(const char *s);

int main(void) {
    puts("Hello, world!");
    return 0;
}
```

The interpreter stops at the call, because it cannot run `puts`:

```console
$ evocc --interpret-c-hir shout.c
shout.c:4:5: error[E3039]: external-call-not-executable
```

This is why the interpreters are tools for checking code, not a way to run whole programs.

## Look at the assembly

`--emit=asm` prints the assembly that EvoCC writes. The first lines for `sum.c` are:

```console
$ evocc --target=aarch64-unknown-linux-gnu --emit=asm sum.c | head -n 24
// evocc-native-ub-checks=checked
.text

.p2align 2
.type sum_to, %function
sum_to:
    stp x29, x30, [sp, #-16]!
    mov x29, sp
.Lsum_to_block0:
    movz w0, #0
    movz w1, #1
.Lsum_to_block1:
    cmp w1, #10
    b.le .Lsum_to_block3
    b .Lsum_to_block4
.Lsum_to_block2:
    movz w3, #1
    adds w3, w1, w3
    b.vs .Lsum_to_block8
.Lsum_to_block6:
    mov w1, w3
    mov w0, w2
    b .Lsum_to_block1
.Lsum_to_block3:
```

Look at `adds` and `b.vs`. The addition `i++` sets the overflow flag, and `b.vs` jumps away if the
addition overflowed. Signed overflow is undefined behavior in C, so EvoCC keeps a check for it.
[Chapter 4](undefined-behavior.md) explains these checks.

On Linux, the system compiler can assemble and link this file. We ran these commands on Linux on
AArch64 with GCC 14:

```console
$ evocc --target=aarch64-unknown-linux-gnu --emit=asm sum.c > sum.s
$ gcc sum.s -o sum
$ ./sum; echo $?
55
```

The native program returns 55, like the two interpreters.

## Build a real project

Most build systems take the C compiler from the variable `CC`. So you can build many projects with
EvoCC without changing them. Here is zlib, the compression library:

```console
$ curl -LO https://zlib.net/zlib-1.3.2.tar.gz
$ tar xzf zlib-1.3.2.tar.gz
$ cd zlib-1.3.2
$ CC=evocc ./configure
$ make
$ make test
```

`make test` runs the tests twice: once with the static library and once with the shared library.
When all tests pass, its output includes these two lines:

```text
		*** zlib test OK ***
		*** zlib shared test OK ***
```

Some Makefiles set `CC` themselves. For those, give the variable on the command line:
`make CC=evocc`.

## Plan and apply repairs

Use `evo fix input.c --plan` to inspect source repairs. Add `--json` for one
JSON object on standard output. The object has `file`, `repairs`,
`diagnostics`, `applied`, and `exit_code`. The repair fields and byte-column
spans are defined in [`spec/diagnostics.md`](https://github.com/onehr/evocc/blob/main/spec/diagnostics.md).

```console
$ evo fix input.c --plan --json
$ evo fix input.c --apply --json
$ evo fix input.c --apply --review --json
```

`--apply` uses only safe repairs. `--review` also permits repairs that can change
behavior. A human must inspect those repairs first. The five repairs remove an
unused declaration, add a void cast for an unused parameter, add a missing
function prototype, write an implicit `int`, or add `return 0;`.

The command parses the file with warnings enabled. It does not run the program.
It will not edit input that has a parse error. After applying edits, it runs the
syntax check again. If that check fails, it restores the original bytes and
exits 1. A successful plan or apply exits 0. A plan, or a file with no selected
repairs, has `applied: 0`. Multiple diagnostics can share one declaration edit.
The command does not edit included files or macro expansions. It leaves out
repairs whose source spelling or required type cannot be expressed. Volatile
accesses and other observable effects are not safe to remove.

The compiler bridge is `evocc --parse-only --repair=plan|safe|review FILE.c`.
Add `--error-format=json` for the same record. Other compiler options can set
include paths and macros. A denied warning prevents this bridge from editing;
the `evo fix` command uses `-Wall --cap-lints=warn` to collect repair candidates.

## Driver options

`evocc` accepts the options of a usual C compiler:

| Option | What it does |
|---|---|
| `-E` | Preprocess only, and write the result. |
| `-S` | Compile to assembly. |
| `-c` | Compile and assemble to an object file. Do not link. |
| `-o FILE` | Write the output to `FILE`. |
| Several input files | Compile each C file, then link all the files together. |
| `-M`, `-MM`, `-MD`, `-MMD` | Write the dependencies for `make`: `-M` and `-MM` in place of the output, `-MD` and `-MMD` beside it. `-MM` and `-MMD` leave out the system headers. |
| `-MF FILE`, `-MT TARGET`, `-MQ TARGET`, `-MP` | Name the dependency file and the target of the rule; add a phony rule for each header. |
| `-shared` | Link a shared library. |
| `-l NAME`, `-L DIR`, `-Wl,ARGS` | Link the library `NAME`; search for libraries in `DIR`; give `ARGS` to the linker. |
| `--target=TARGET` | Make code for `aarch64-apple-darwin` or `aarch64-unknown-linux-gnu`. The default is the host. |
| `-I DIR`, `-D NAME[=VALUE]`, `-U NAME`, `-include FILE`, `-isystem DIR` | Control the preprocessor, as in GCC. |
| `-std=gnu11` and other `-std=` values | Select the language standard. |
| `-O0` to `-O3` | Select the optimization level. `-O0` keeps the control flow of the source. |
| `-g` | For `aarch64-unknown-linux-gnu`, write debug information about C types (DWARF). EvoCC does not write line tables yet. For `aarch64-apple-darwin`, `-g` has no effect. |
| An unknown `-f` or `-m` option | EvoCC prints one warning and continues. `-Werror` makes this an error so build-system probes do not report unsupported features as available. `-w` turns the warning off. |
| `-fstack-protector`, `-fstack-protector-strong` | Both enable EvoCC's existing strong stack-protection policy. `-fno-stack-protector` disables it. |
| `-coverage`, `--coverage`, `-fprofile-arcs`, `-ftest-coverage` | Print the unsupported-option warning; `-Werror` makes it an error and `-w` turns it off. No instrumentation or `.gcno`/`.gcda` files are produced. |
| `--version`, `-dumpmachine`, `--help` | Print the version and the target, the target only, or a summary of the options. |

EvoCC also has its own options. You meet them in the next chapters:

| Option | What it does | Chapter |
|---|---|---|
| `--interpret-c-hir`, `--interpret-evoir` | Run `main` in an interpreter. | 3 |
| `--check` | Report undefined behavior on the path that runs. | 4 |
| `--explain CODE` | Explain a diagnostic code. | 4 |
| `--error-format=json` | Write diagnostics as JSON, one record per line. | 4 |
| `--emit=c-hir`, `--emit=evoir`, `--emit=asm` | Print a representation of the program. | 4, 7 |
| `--proof-required=DIR` | Compile with proof. | 5 |
| `-Oeq-scalar` | Run the checked optimizer. | 6 |
