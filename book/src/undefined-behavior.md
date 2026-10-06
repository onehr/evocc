# Undefined Behavior

C leaves some operations undefined. A signed integer can overflow. A program can divide by zero,
read a variable that it never wrote, or read past the end of an array. The C standard does not say
what happens then. A usual compiler may assume that these things never happen, and when they do
happen, the program can do anything, often with no sign of a problem.

EvoCC makes undefined behavior explicit. Its interpreters stop at undefined behavior and give it a
name and a number. In the compiled program, each condition that C requires stays as a check. This
chapter shows both.

## Four mistakes

Here are four small programs. Each has one mistake. Save each in the file named above it.

`overflow.c` multiplies past the largest `int`:

```c
int scale(int x) {
    return x * 1000;
}

int main(void) {
    int big = 3000000;
    return scale(big) > 0;
}
```

`divide.c` divides by zero:

```c
int average(int total, int count) {
    return total / count;
}

int main(void) {
    return average(10, 0);
}
```

`uninit.c` reads a variable that it never wrote:

```c
int main(void) {
    int score;
    int bonus = 2;
    return score + bonus;
}
```

`bounds.c` reads one element past the end of an array, because the loop uses `<=`:

```c
int main(void) {
    int days[7] = {1, 2, 3, 4, 5, 6, 7};
    int sum = 0;
    for (int i = 0; i <= 7; i++)
        sum += days[i];
    return sum;
}
```

## Find them with `--check`

`--check` runs `main` in the EvoIR interpreter. It stops at the first undefined behavior on the
path that runs:

```console
$ evocc --check overflow.c
overflow.c: error[E3004]: signed-integer-overflow
$ evocc --check divide.c
divide.c: error[E3002]: integer-division-by-zero
$ evocc --check uninit.c
uninit.c: error[E3001]: read-of-uninitialized-object
$ evocc --check bounds.c
bounds.c: error[E3011]: out-of-bounds-memory-access
```

Each diagnostic has a code, such as `E3004`, and a name, such as `signed-integer-overflow`. The
exit status is 1:

```console
$ evocc --check overflow.c; echo $?
overflow.c: error[E3004]: signed-integer-overflow
1
```

These diagnostics have no line number yet, because the EvoIR interpreter does not keep source
positions. The C-HIR interpreter does. Use it to find the line and the column:

```console
$ evocc --interpret-c-hir overflow.c
overflow.c:2:12: error[E3004]: signed-integer-overflow
$ evocc --interpret-c-hir divide.c
divide.c:2:12: error[E3002]: integer-division-by-zero
$ evocc --interpret-c-hir uninit.c
uninit.c:4:12: error[E3001]: read-of-uninitialized-object: score
$ evocc --interpret-c-hir bounds.c
bounds.c:5:16: error[E3011]: out-of-bounds-memory-access
```

The C-HIR interpreter also names the variable that `uninit.c` reads, `score`.

## What a code means

`--explain` prints the entry of a code:

```console
$ evocc --explain E3004
E3004: signed-integer-overflow
Detected: runtime
Authority: spec/evoir.md, section 6.3 Diagnostic Authority.

A runtime finding describes an executed path, not every possible input.
Resource limits and unsupported execution do not establish absence of UB.
```

The entry names its authority: the table of diagnostics in the EvoIR specification. The
[EvoIR specification, section 6.3](https://github.com/onehr/evocc/blob/main/spec/evoir.md#63-diagnostic-authority)
lists every code.

## Diagnostics for tools

Editors and scripts can read diagnostics as JSON. `--error-format=json` writes one record per line:

```console
$ evocc --check --error-format=json overflow.c
{"schema":"evocc.diagnostic.v1","kind":"diagnostic","severity":"error","code":"E3004","message":"signed-integer-overflow","phase":"execution","category":"undefined-behavior","source":"overflow.c","location":null,"explanation":"available"}
```

The record names the phase in which EvoCC found the problem and its category.
`"location":null` says that this diagnostic has no line and column. When the run completes with no
finding, `--check` writes a record of the result instead. Here is `sum.c` from chapter 3:

```console
$ evocc --check --error-format=json sum.c
{"schema":"evocc.check.v1","kind":"check","status":"completed","main_return":"55","claim":"executed-path-only"}
```

## What `--check` does not tell you

Read the claim in the last record: `executed-path-only`. `--check` runs your program once, on the
path that its inputs select. A clean run does not prove that other inputs are safe. If the
interpreter reaches its step limit or a call that it cannot run, the check is incomplete. It is not
a pass.

## Undefined behavior in the IR

To see how the compiler keeps undefined behavior in view, print the EvoIR of `scale`:

```console
$ evocc --emit=evoir overflow.c | head -n 10
func @scale(%arg0: bits<32>, %arg1: mem<all>, %arg2: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, fenv=false, may_trap=true, may_unwind=false }
{
^entry(%v0: bits<32>, %v1: mem<all>, %v2: world):
    %v3 = const.bits<32> 1000
    %v4 = signed_mul_defined %v0, %v3
    ub.assume.signed_overflow %v4
    %v5 = mul.bits<32> %v0, %v3
    ret %v5, %v1, %v2
}
```

Three lines do the work:

- `signed_mul_defined %v0, %v3` computes a truth value: is `x * 1000` defined for this `x`?
- `ub.assume.signed_overflow %v4` is an *obligation*. It states that the program requires this
  truth value to be true.
- `mul.bits<32> %v0, %v3` is the multiply itself. It is a plain bit-vector operation with no
  hidden assumption.

The obligation is a separate instruction. An optimization can remove the multiply, but it cannot
remove the obligation. In many compilers, this fact is a flag on the multiply, and a later pass can
drop it.

## The check in the machine code

The backend turns each obligation into a check. Here is the start of the assembly for `scale`:

```console
$ evocc --target=aarch64-unknown-linux-gnu --emit=asm overflow.c | head -n 21
// evocc-native-ub-checks=checked
.text

.globl scale
.p2align 2
.type scale, %function
scale:
    stp x29, x30, [sp, #-16]!
    mov x29, sp
.Lscale_block0:
    movz w1, #1000
    smull x2, w0, w1
    sbfx x3, x2, #0, #32
    cmp x2, x3
    b.ne .Lscale_block1
.Lscale_block2:
    mul w0, w0, w1
    ldp x29, x30, [sp], #16
    ret
.Lscale_block1:
    brk #3004
```

`smull` computes the full 64-bit product. `sbfx` and `cmp` test whether it fits in 32 bits. If it
does not fit, the program jumps to `brk #3004`. The number is the diagnostic code `E3004`. We ran
the program on Linux on AArch64:

```console
$ evocc --target=aarch64-unknown-linux-gnu --emit=asm overflow.c > overflow.s
$ gcc overflow.s -o overflow
$ ./overflow; echo $?
Trace/breakpoint trap
133
```

The program stops at the overflow, with the signal `SIGTRAP`, and the shell reports the signal.
The exact text of that report depends on your shell. The program does not return a wrong number.

## When you want wrapping arithmetic

Some programs rely on signed arithmetic that wraps around, as on the machine. GCC and Clang have
the option `-fwrapv` for them, and so does EvoCC. With `-fwrapv`, a signed overflow is defined, so
the obligation goes away:

```console
$ evocc --emit=evoir -fwrapv overflow.c | head -n 8
func @scale(%arg0: bits<32>, %arg1: mem<all>, %arg2: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, fenv=false, may_trap=false, may_unwind=false }
{
^entry(%v0: bits<32>, %v1: mem<all>, %v2: world):
    %v3 = const.bits<32> 1000
    %v4 = mul.bits<32> %v0, %v3
    ret %v4, %v1, %v2
}
$ evocc --interpret-c-hir -fwrapv overflow.c
0
```

The product wraps to a negative number, so `scale(big) > 0` is false and `main` returns 0. The
effect summary now says `may_trap=false`.

`-fwrapv` changes the meaning of your program. Use it when the program needs it, not to hide a
diagnostic.
