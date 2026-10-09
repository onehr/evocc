# Silent undefined behavior corpus

This corpus has 18 silent memory defects and six integer-UB controls.
Each file has one injected defect. These are not bugs found in upstream code.
The memory items adapt six kernels from zlib 1.3.2, Zstandard 1.5.7 and
cJSON 1.7.19. Each kernel is used with three different memory defects.
The six controls are written for this corpus. The upstream notices are kept
in LICENSE.zlib, LICENSE.zstd and LICENSE.cjson. Each origin.md names its source.

The program returns a processed count. A library caller can read a checksum
through evaluate. The hidden driver checks that checksum for twelve seeds.
Expected outputs were calculated independently with unsigned modular integer
arithmetic, then checked against fixed.c on three sealed compiler builds.
Only the program and its intended-behavior comments are visible to the agent.

qualification.json records five executions per build and the exact C1/C3
feedback. All 24 fixed programs pass GCC 14 O0/O2 and Clang 19 O2. Each faulty
program fails at least one hidden build. All 18 silent programs have the same
C1 main output and exit code before and after repair, with no C1 warning or
runtime finding. EvoCC reports each requested code at the single changed line.

Two candidates were replaced once before model use. The first empty-mean
contract matched the native division-by-zero result. A remainder candidate
reported E3003 instead of the requested E3002. Their failed records remain
under replaced. There is no class shortfall.

The detection matrix is line-specific: reported means a finding at the fix
span, not any finding in the program. In particular, MemorySanitizer reports
uninitialized values at later uses in the reset items. A false matrix cell
does not claim that tool cannot detect that defect. Compiler errors and
unavailable sanitizer runs are not negative detection results.

The shared kernels and defect scaffolding limit statistical independence.
This is a fixed, selected corpus, not a sample of arbitrary C programs.
