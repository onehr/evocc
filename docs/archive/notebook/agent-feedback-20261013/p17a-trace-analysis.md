# P17a: the three EvoCC response failures of P17

This note examines the retained traces of the three P17 trials where EvoCC
feedback (condition C3) ended without a scored repair. It answers four
questions: what the model saw, how long that feedback was, where the response
budget went, and whether a shorter E3004 text or a review-only overflow plan
could plausibly change the result. It separates output delivery from
hidden-test correctness. It changes no code, evidence or task file.

## Sources and method

- Repository commit at analysis time: `6b1e0f10`. Frozen compiler:
  `.build/frozen/366fd796/evocc-darwin` (`evocc 0.1.0 (366fd796613e 2026-10-13)`).
- P17 receipt `tests/contracts/agent-repair-20261008.json`, SHA-256
  `26132a5205389c4414d0e07ef39600039a10fd0a02a09ff3405c3f6413dd4c96`.
- P17 transcripts `tests/contracts/agent-repair-transcripts-20261008.tar.gz`, SHA-256
  `226a490f2365782314bf88a0e251ebe7bff15a6a6e2547c3cb13b8dd3c1731b8`.
  The archive was unpacked outside the repository. Each transcript keeps the
  feedback, the request and the raw HTTP response, including `reasoning`.
- Corpus at tag `agent-repair-corpus-2` (`7eed5eac`), five E3004 items under
  `tests/fixtures/agent-repair/ub-*-overflow/`.
- No model or API request was made. No Cargo command ran. The frozen compiler
  ran only for the model-free check in "Output delivery and correctness".
- Characters are Unicode code points of the `reasoning` string. Token counts
  come from the provider's `usage` record.

## Findings

1. All three trials stopped at the 4,096-token completion limit
   (`finish_reason=length`). Reasoning used 4,096, 4,096 and 4,007 tokens.
   Two trials returned no answer text. One returned 89 tokens of a C block.
2. Each trace found a correct saturating repair early: in the first 8.7%,
   4.1% and 20.5% of its reasoning. The rest went to deliberation, mostly
   about keeping or extending `main` and about integer widths and literals.
3. Reasoning that cites the EvoCC record used 16%, 20% and 6% of the three
   traces. The E3004 page example uses `long` and `2147483648L`; two traces
   debated `long` against `long long`. One trace read the record's
   `"exit_code":1` as the program's exit status.
4. Output delivery failed in 3/3 trials. Hidden-test correctness was not
   observed in these trials. All 37 delivered responses on the five overflow
   items passed the hidden test, in every condition.
5. The last complete program draft in each trace passes all hidden inputs
   under the frozen EvoCC interpreter. This is a model-free counterfactual,
   not a P17 result.
6. GCC (C1) and Clang (C2) feedback on these items differ by one summary line.
   Their length stops were 0/15 and 5/15. Provider routing was not controlled,
   and it is strongly associated with response length.
7. A shorter E3004 text could plausibly change one or two of the three
   outcomes. The traces cannot establish it. A larger response budget would
   most likely have delivered all three repairs. A review-only plan would not
   supply missing knowledge; the model already had the repair.

## The three trials

| Receipt row | Transcript | Item | Repetition | Provider | Prompt tokens | Reasoning tokens | Answer tokens | Harness label |
| ---: | --- | --- | ---: | --- | ---: | ---: | ---: | --- |
| 60 | `0060.json.gz` | `ub-addition-overflow` | 1 | AtlasCloud | 784 | 4,096 | 0 | `malformed_response` |
| 69 | `0069.json.gz` | `ub-negation-overflow` | 1 | Alibaba | 786 | 4,096 | 0 | `malformed_response` |
| 266 | `0266.json.gz` | `ub-subtraction-overflow` | 3 | Alibaba | 796 | 4,007 | 89 | `invalid_candidate:fenced_file` |

Each trial was round 1 of its repetition, with HTTP status 200 and no retry.
The harness label `malformed_response` means that the answer text was empty.
It does not mean that the HTTP JSON was malformed. The label
`invalid_candidate:fenced_file` means that the C block had no closing fence.
The three requests cost USD 0.002420688, 0.0025755 and 0.002577.

## What the model saw

The system message was `tools/experiment/prompt.txt` (732 bytes). The user
message was the current `program.c` and the line `Feedback:`, then three JSON
records, one per line:

| Record | Bytes in trial 60 | Content |
| --- | ---: | --- |
| `evo check --json` | 730 | One E3004 diagnostic. `rendered` repeats the message, the absolute scratch path, the source line, the caret label and the note. `spans` repeats the path and the location. |
| `evo explain E3004 --json` | 772 | The full page: heading, two paragraphs, a failing and a fixed example, and a closing sentence. Newlines are escaped as `\u000a`. |
| `evo fix --plan --json` | 152 | An empty plan: no repair, no diagnostic, `applied` 0. |
| Total feedback | 1,654 | Trial 69: 1,648. Trial 266: 1,654. |

The E3004 page lines that the traces discuss are:

```text
Check the range before the operation, or use a suitable wider arithmetic type.
...
    long result = (long)value + 1;
    return result != 2147483648L;
...
The conversion must occur before addition, not after an overflowing result.
```

For `ub-addition-overflow`, the same item had 351 prompt tokens with GCC
feedback (283 bytes) and 368 with Clang feedback (354 bytes). EvoCC feedback
added 433 tokens: about 3.17 bytes per token for this JSON. Prompt length does
not consume the completion budget. `max_tokens` limits completion tokens only.

## Where the budget went

For this model and route, `max_tokens=4096` counts reasoning tokens. The
`usage.completion_tokens_details.reasoning_tokens` field shows that reasoning
took the whole budget in trials 60 and 69 and 4,007 tokens in trial 266.

The reasoning strings have 16,018, 14,749 and 14,962 characters. I split each
string at blank lines into paragraphs: 42, 42 and 50 of them. I read every
paragraph and gave it one label by its main topic. The table gives the share
of reasoning characters for each label.

| Label | Trial 60 | Trial 69 | Trial 266 |
| --- | ---: | ---: | ---: |
| First analysis and repair design | 9% | 7% | 23% |
| Keep `main` unchanged or extend it | 39% | 14% | 36% |
| Integer widths, `INT_MIN` spelling, C dialect | 18% | 40% | 21% |
| Reasoning that cites the EvoCC record | 16% | 20% | 6% |
| Prompt rules: markers, includes, hard-coding | 5% | 2% | 5% |
| Code drafts | 8% | 13% | 6% |
| Re-checking a finished draft | 5% | 3% | 4% |

The labels, in paragraph order, are recorded here so that a reviewer can
check them. D is design, M is `main`, W is widths and literals, F cites the
record, R is prompt rules, C is a code draft and V is re-checking.

```text
0060: D M M R C C W W W F M M M M M M W M F M R W F R C W W C W M F V V V V V V R C C C C
0069: D M W C V F R M M C R W F V V V C C F C W C W F W R M M C C C C R W W F F F F W W W
0266: D D D D D W M M M W R F C C C W W R M M M W M M F R W W W R F W W W V M M M F C C C C C R V W W W V
```

Facts from the traces:

- A correct repair design appears at character 1,387 (8.7%), 606 (4.1%) and
  3,072 (20.5%). The first complete function definition appears at 27.5%,
  15.4% and 49.9%.
- The traces wrote 3, 7 and 2 complete definitions of the repaired function.
  Trial 69 changed between a branch on `INT_MIN` and a `long long` clamp,
  then began a third version for a hypothetical 64-bit `int`.
- Trial 266 wrote the whole program in a fenced block inside its reasoning,
  re-checked it for about 1,000 characters, and then began the answer. The
  answer text is the first 315 bytes of that 489-byte draft.
- The prompt sentence "Preserve the main function and make it exercise the
  intended behavior" caused the longest single topic in trials 60 and 266.
  The prompt is the same in every condition.

Reasoning that cites the EvoCC record:

- Trial 60, paragraph 22 (1,252 characters): the page example uses `long`.
  The trace asks whether `long` is 32 bits on Windows, whether `long long`
  needs C99, and whether the checker uses `-m32`.
- Trial 69, paragraphs 23 and 37: the trace says that a `long long` version
  "aligns with the repair hint" and "follows the hint", the hint being "use a
  suitable wider arithmetic type". The trace then left its first correct
  branch for that version. Paragraphs 12 and 35 use the reported executed
  path as evidence that `int` has 32 bits.
- Trial 266, paragraphs 0, 8, 20 and 38: the trace reads `"exit_code":1` in
  the check record as the program's exit status. The field is the status of
  the check command. The word `exit_code` occurs six times in this trace.
- All three traces also ask what the checker would report for the repaired
  code.

A keyword screen over all 45 round-1 responses on the five overflow items
gives a similar picture. Paragraphs that mention widths or literals cover
47% to 65% of the reasoning in every condition, including GCC and Clang.
Paragraphs that mention the feedback cover 45% to 49% of the length-stopped
responses in C2 and C3, and 13% to 23% of the delivered ones. This screen is
crude and correlational.

## Output delivery and correctness

Output delivery means a complete fenced C block before the limit. Hidden-test
correctness means that the delivered candidate passes the registered scorer.

| Overflow items, round 1 | Responses | Delivered | Hidden pass among delivered | Median completion tokens | Responses with 3,000 or more completion tokens |
| --- | ---: | ---: | ---: | ---: | ---: |
| C1 GCC | 15 | 15 | 15 | 1,545 | 3 |
| C2 Clang | 15 | 10 | 10 | 2,851 | 7 |
| C3 EvoCC | 15 | 12 | 12 | 1,938 | 6 |

Over all 30 P17 items, 259 of 272 responses were delivered and 257 of those
passed. The three C3 failures are delivery failures. Their correctness is
unknown, not negative.

A model-free counterfactual checks the last complete program draft in each
trace. Trial 60 ends inside `main`; its draft keeps the original `main`, as
the trace states, and the cut text is a prefix of that `main`. Trials 69 and
266 have complete drafts. Each draft was joined to a driver that compares the
hidden reference inputs with `expected.txt` and returns the mismatch count.
`evocc-darwin --check` reported `main returned 0` and no undefined behavior
for all three. The same driver returns 0 for each `fixed.c`. It reports E3004
for the original addition item and returns 2 when one clamp value is wrong.
This uses the EvoCC interpreter, not the registered sealed GCC 14 scorer. It
does not change any P17 count.

## Other conditions and the provider

- C1 and C2 send almost the same text for these items: the UBSan runtime
  line, with one more `SUMMARY:` line in C2. Their length stops were 0/15
  and 5/15. Feedback text alone does not explain that difference.
- Over all items, length stops were 2/91 calls in C1, 8/91 in C2 and 3/90
  in C3.
- OpenRouter chose the provider for each request. On the overflow items,
  Together served 10 responses with a median of 449.5 completion tokens and
  no length stop. The other providers served 35 responses with a median of
  2,560 and 8 length stops. The three C3 failures came from AtlasCloud and
  Alibaba.
- Experiment 2 (`agent-repair-2-20261009.json`) used a 16,384-token limit on
  a larger corpus. DeepSeek exhausted it in 252 of 403 answered calls, in all
  five conditions. With no feedback at all (C0), it exhausted it in 63 of 80.

## Could shorter guidance or a review plan change the result?

Shorter E3004 guidance:

- The input length did not use the completion budget. Only a change in the
  model's deliberation could change delivery.
- The cited-record share was 16%, 20% and 6%, about 650, 820 and 240 tokens.
  Trial 266 needed about 60 more tokens to close its answer. Trial 60 was
  writing its final draft when it stopped. A text without the `long` example
  and without the ambiguous `exit_code` reading could plausibly have saved
  enough tokens in these two trials. This assumes that the other topics stay
  unchanged, and the traces cannot test that assumption.
- Trial 69 did not converge. A shorter text would probably not have saved it.
- The largest topics (keeping `main`, widths and literals) also occur with
  GCC and Clang feedback. A shorter E3004 text cannot remove them.

An intent-specific, human-reviewed overflow plan:

- The plan was empty in all three trials. The model did not lack the repair;
  it found it within the first 21% of its reasoning.
- A concrete reviewed replacement could shorten the width and literal topic,
  because it shows `long long` and the spelling `-2147483647 - 1`. It cannot
  shorten the topic about `main`, which comes from the prompt.
- Saturation changes program behavior. The plan must stay a `review` repair
  that names its intent. It must never become a safe or automatic repair.

The response budget:

- All 37 delivered overflow responses passed. On these items the observed
  failure mode is delivery, not repair quality.
- A larger registered budget would most likely have delivered all three
  repairs. Experiment 2 shows that a larger budget alone does not remove
  exhaustion on harder items.

## Consequences for a registered follow-up

- Pin the provider and record the reported provider for every response.
- Register the response budget. Measure the full completion length under a
  large budget; delivery at 4,096 then follows from the same trials.
- Report delivery, candidate validity and hidden-test correctness separately.
- Change one thing per arm: the E3004 text first; the review plan only in a
  separate arm.
- Use single-response trials. All three failures were in round 1.
- `protocols.md` in this directory gives the proposal, the guidance text and
  five reviewed-plan drafts that pass the hidden inputs model-free.

## Reproduce without a model request

1. Unpack the transcript archive into an empty directory outside the
   repository. Read it with `python3 -I`.
2. In `0060.json.gz`, `0069.json.gz` and `0266.json.gz`, element 0 holds
   `feedback`. Element 1 holds `request` and `http[0].response`. Strip the
   leading whitespace from the response, parse it, and read
   `choices[0].message.reasoning`, `.content` and `usage`.
3. Split `reasoning` with the regular expression `\n\s*\n` and apply the
   label strings above in order.
4. For the condition table, read every transcript entry that has `request`
   and group the responses of the `ub-*-overflow` items by condition.

## Limits

- Three traces cannot estimate an effect. The paragraph labels are one
  reader's judgment; the label strings make them reviewable.
- Each trial used one provider-chosen route at temperature 0. P17 shows that
  routes differ strongly in response length.
- The counterfactual check uses a different executor than the registered
  scorer. It shows only that the drafts were correct on the hidden inputs.
- This note makes no claim about other diagnostics, models or budgets.
