# Undefined Behavior

C leaves some operations undefined. A signed integer can overflow. A program can divide by zero,
read a variable that it never wrote, or read past the end of an array. The C standard does not say
what happens then. A usual compiler may assume that these things never happen, and when they do
happen, the program can do anything, often with no sign of a problem.

EvoCC keeps each supported undefined-behavior obligation as an explicit check. Its interpreters
stop at the first check that fails and report a diagnostic. Checked native code keeps the
supported scalar checks.

## Four mistakes

Each of these four programs has one mistake. Save each in the file named above it.

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

## Find them with `evo check`

`evo check` runs `main` with `evocc --check`. It reports the first undefined operation on the
executed path. Each command below exits with status 1:

```console
$ evo check overflow.c
error[E3004]: signed-integer-overflow
  --> overflow.c:2:12
   |
 2 |     return x * 1000;
   |            ^^^^^^^^ signed-integer-overflow
   |
   = note: this operation failed on the executed path
$ evo check divide.c
error[E3002]: integer-division-by-zero
  --> divide.c:2:12
   |
 2 |     return total / count;
   |            ^^^^^^^^^^^^^ integer-division-by-zero
   |
   = note: this operation failed on the executed path
   = help: check the divisor before the division or remainder operation
$ evo check uninit.c
error[E3001]: read-of-uninitialized-object
  --> uninit.c:4:12
   |
 4 |     return score + bonus;
   |            ^^^^^^^^^^^^^ read-of-uninitialized-object
   |
   = note: this operation failed on the executed path
$ evo check bounds.c
error[E3011]: out-of-bounds-memory-access
  --> bounds.c:5:16
   |
 5 |         sum += days[i];
   |                ^^^^^^^ out-of-bounds-memory-access
   |
   = note: this operation failed on the executed path
```

The arrow gives the file, line and byte column. The label marks the failing expression.

## What a code means

`evo explain` prints the tested explanation of a code:

```console
$ evo explain E3004
# E3004: signed-integer-overflow

An executed signed integer operation has a result outside its permitted range.
Negating the minimum value and dividing it by minus one can also cause overflow.

The checked profile reports overflow instead of assuming signed arithmetic wraps.
Check the range before the operation, or use a suitable wider arithmetic type.

int main(void) { int value = 2147483647; return value + 1; }

int main(void) {
    int value = 2147483647;
    long result = (long)value + 1;
    return result != 2147483648L;
}

The conversion must occur before addition, not after an overflowing result.
```

The [Error Index](error-index.md) holds the explanation of every code. The table in
[section 6.3 of the EvoIR specification](https://github.com/onehr/evocc/blob/main/spec/evoir.md#63-diagnostic-authority)
is the authority for each code.

## Diagnostics for tools

Editors and scripts can read diagnostics as JSON. `--error-format=json` writes one record per line:

```console
$ evocc --check --error-format=json overflow.c
{"code":"E3004","level":"error","message":"signed-integer-overflow","spans":[{"file":"overflow.c","line_start":2,"column_start":12,"line_end":2,"column_end":20,"label":"signed-integer-overflow","primary":true}],"notes":["this operation failed on the executed path"],"help":[],"rendered":"error[E3004]: signed-integer-overflow\n  --> overflow.c:2:12\n   |\n 2 |     return x * 1000;\n   |            ^^^^^^^^ signed-integer-overflow\n   |\n   = note: this operation failed on the executed path\n"}
```

The record contains source spans, notes, help and the rendered text. When the run completes with no
finding, `--check` writes a record of the result instead. Here is `sum.c` from
[Hello, World!](hello.md#run-a-program-without-compiling-it):

```console
$ evocc --check --error-format=json sum.c
{"schema":"evocc.check.v1","kind":"check","status":"completed","main_return":"55","claim":"executed-path-only","scope":"No finding from the implemented checks on the executed path. Pointer subtraction and ordering obligations are not checked. This does not check that the results are correct."}
```

## What `--check` does not tell you

Read the claim in the last record: `executed-path-only`. `--check` runs your program once, on the
path that its inputs select. A clean run does not prove that other inputs are safe. `--check` does
not check the same-object or exact-element obligations of pointer subtraction, or same-object
ordering. If the interpreter reaches its step limit or a call that it cannot run, the check is
incomplete. It is not a pass.

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

- `signed_mul_defined %v0, %v3` computes a truth value: is `x * 1000` defined for this `x`?
- `ub.assume.signed_overflow %v4` is an *obligation*. It states that the program requires this
  truth value to be true.
- `mul.bits<32> %v0, %v3` is the multiply itself. It is a plain bit-vector operation with no
  hidden assumption.

The obligation is a separate instruction. An optimization can remove the multiply, but it cannot
remove the obligation. In many compilers, this fact is a flag on the multiply, and a later pass can
drop it.

## The check in the machine code

The backend turns each obligation into a check:

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
