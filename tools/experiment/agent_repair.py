#!/usr/bin/env python3
"""Silent-UB repair experiment: the Kimi Code CLI repairs one C program per session (one item, one condition).
The agent runs under a Seatbelt sandbox with only program.c and an opaque `check` that builds and runs any
C file in the sealed container; only the diagnostic layer differs. Uses three sealed scoring builds."""
import argparse, datetime, hashlib, json, os, re, shlex, shutil, signal, subprocess, sys, tarfile, time, urllib.request, uuid
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import repair  # noqa: E402  scoring, diff measure, host run and sealed container of the second experiment
import qualify_silent

ROOT, KIMI, KHOME, UHOME = repair.ROOT, Path.home() / ".kimi-code/bin/kimi", Path.home() / ".kimi-code", Path.home()
EVO, EVOCC = repair.EVO, repair.EVO.with_name("evocc")
SYSROOT = ROOT / "sysroot/aarch64-unknown-linux-gnu"
TOOLS = "sh bash cat ls head tail sed awk grep diff wc sort uniq cut tr echo printf cp mv rm mkdir test true false sleep".split()
EXTRA_TOOLS = ["rg"]  # Kimi's Grep and Glob call rg from PATH; without it Kimi downloads rg into its home.
TOKENS = ("inputOther", "inputCacheRead", "inputCacheCreation", "output")
AGENT_FILE = ("---\nname: evocc-repair\ndescription: The default Kimi Code agent without web tools and without sub-agents.\n"
              "disallowedTools: WebSearch, FetchURL, Agent, AgentSwarm\n---\n${base_prompt}\n")
LOGIN_SHELL = "#!/bin/sh\nexec /usr/bin/env\n"  # Kimi runs "$SHELL -l -c /usr/bin/env" once; this reads no profile.
WEB, FILE_TOOLS = {"WebSearch", "FetchURL", "WebFetch"}, {"Read", "Write", "Edit", "Grep", "Glob", "ReadMediaFile"}
# A compiler, interpreter or network tool. The sandbox blocks these; the audit flags any that ran.
FORBIDDEN = re.compile(r"^(cc|c\d\d|gcc|g\+\+|c\+\+|clang|clang\+\+|cpp|tcc|cc1|zig|xcrun|as|ld|evo|evocc|"
                       r"python\d?|python\d\.\d+|perl|ruby|node|deno|bun|osascript|swift|swiftc|java|"
                       r"curl|wget|nc|ncat|netcat|ssh|scp|sftp|telnet|ftp|git|open|nscurl)$")
DENIED = re.compile(r"(?i)operation not permitted|not permitted|permission denied|command not found|"
                    r"no such file or directory|cannot execute|killed|sandbox")
STOP = re.compile(r"(?i)rate.?limit|quota|balance|unauthori|forbidden|provider\.\w+: (40[123]|429)\b")
HEREDOC = re.compile(r"(<<-?[ \t]*(['\"]?)(\w+)\2[^\n]*)\n(?:.*?\n)??[ \t]*\3[ \t]*(?=\n|$)", re.S)
SEPARATORS = {";", "&&", "||", "|", "&", "(", ")", ";;", "|&"}
PREFIXES = {"env", "command", "exec", "time", "nice", "nohup", "sudo", "xargs", "builtin", "then", "do", "else", "if", "while", "until", "!"}
CAPTURE = 64 * 1024 ** 2
# Protocol B (P22). F is the current full-1 feedback of c3. K is the same header and one concise-1 check record.
# T03s (2026-10-16) corrected the completion scope of a clean check in both forms. F is therefore the frozen
# feedback with exactly this correction; every other byte must still match. The owner approved this on 2026-10-20.
OLD_SCOPE = "No undefined behavior was found on the executed path. This does not check that the results are correct."
NEW_SCOPE = ("No finding from the implemented checks on the executed path. Pointer subtraction and ordering "
             "obligations are not checked. This does not check that the results are correct.")
F_K = ("c3", "k")
MARK = "=== evo diagnostics\n"
# The pilot keeps the item order of the protocol text. The rotation then runs F first for items 1, 3 and 5.
PILOT_B = ("adler-byte", "crc-reset", "minify-view", "increment-limit", "mean-empty")
# The agent settings and quota limits that Protocol B fixes. They are the experiment 5 settings.
SETTINGS_B = dict(model="kimi-code/k3", effort="low", wall=900, max_steps=100, prompt="prompt-3.txt",
                  max_week_ratio=0.85, max_week_increase=0.15)
GO_B = ("Zero escapes.",
        'Every K check result with EvoCC output starts its EvoCC part with `{"form":"concise-1"`.',
        "When the agent checks the unchanged `program.c`, the EvoCC part of the F result equals the frozen string for that "
        "source after path replacement and the T03s scope correction.",
        "Every quota read succeeds. The harness records steps, tokens and scores.",
        "There is no unknown outcome.")
GIB = 1024 ** 3
DISK_WAIT = (300.0, 21600.0)  # Protocol B checks the free disk every 5 minutes, for up to 6 hours
# The agent's check command. It is opaque: it holds no harness, repository or run-directory path. It finds
# the workspace .check directory by walking up from the current directory, and relays the file name.
CHECK_CLIENT = r"""#!/bin/sh
set -u
dir=$PWD
while [ ! -d "$dir/.check/req" ]; do
  [ -z "$dir" ] && { echo "check: run this inside the working directory" >&2; exit 2; }
  dir=${dir%/*}
done
file=${1:-program.c}
id="$$-$(awk 'BEGIN{srand(); print int(rand()*1000000000)}')"
printf '%s\n' "$file" > "$dir/.check/req/$id.file"
: > "$dir/.check/req/$id.ready"
i=0
while [ ! -f "$dir/.check/resp/$id.done" ]; do
  sleep 1; i=$((i + 1))
  [ "$i" -gt 900 ] && { echo "check: timed out" >&2; exit 3; }
done
cat "$dir/.check/resp/$id.out"
exit "$(cat "$dir/.check/resp/$id.code" 2>/dev/null || echo 0)"
"""
# Builds and runs one C file in the sealed, network-disabled container. Mode c3 assembles a prebuilt input.s.
CONTAINER_BUILD = r"""ulimit -t 25 2>/dev/null; ulimit -f 1048576 2>/dev/null
cd "$(mktemp -d)" || exit 70
cp /in/input.c input.c 2>/dev/null
mode=$1
if [ "$mode" = c3 ]; then
  cp /in/input.s input.s 2>/dev/null
  if gcc-14 -O0 input.s -o prog 2>err; then rc=0; else rc=$?; fi
  printf '=== assemble and link exit %s\n' "$rc"; sed -n 1,80p err
else
  [ "$mode" = c1 ] && f='-Wall -Wextra -fsanitize=undefined' || f=''
  if gcc-14 -O0 $f -o prog input.c 2>err; then rc=0; else rc=$?; fi
  printf '=== build exit %s\n' "$rc"; sed -n 1,200p err
fi
if [ "$rc" = 0 ]; then
  ./prog >out 2>rerr; pr=$?
  printf '=== run exit %s\n--- stdout\n' "$pr"; sed -n 1,400p out
  printf -- '--- stderr\n'; sed -n 1,400p rerr
fi
"""


def now(): return datetime.datetime.now(datetime.timezone.utc).isoformat(timespec="seconds")
def digest(path): return hashlib.sha256(Path(path).read_bytes()).hexdigest()
def executable(path, text): path.write_text(text); path.chmod(0o755)


def kill(proc):
    try: os.killpg(proc.pid, signal.SIGTERM); proc.wait(10)
    except (subprocess.TimeoutExpired, ProcessLookupError):
        try: os.killpg(proc.pid, signal.SIGKILL); proc.wait()
        except ProcessLookupError: pass


def profile(execs, ws, tmp, home, binp, out, siblings):
    """A Seatbelt profile. Exec uses an allowlist. Reads deny the sensitive trees, including the output
    tree and the other sessions. Writes confine to the workspace, the session temp and the Kimi home."""
    allow_exec = sorted(set(execs) | {"/usr/bin/env", str(KIMI)})
    wr = [str(ws), str(tmp), str(home), str(KHOME), "/private/var/folders"]
    deny_read = [str(ROOT), str(UHOME), "/etc", "/private/etc", str(out)] + [str(p) for p in siblings]
    allow_read = [str(KHOME), str(KIMI.parent), str(ws), str(tmp), str(home), str(binp)]
    sub = lambda xs: " ".join('(subpath "%s")' % x for x in sorted(set(xs)))
    lit = lambda xs: " ".join('(literal "%s")' % x for x in sorted(set(xs)))
    return "\n".join([
        "(version 1)", "(allow default)", "(deny process-exec*)",
        "(allow process-exec*\n  %s)" % lit(allow_exec),
        "(deny file-write*)",
        "(allow file-write*\n  %s %s)" % (sub(wr), lit(["/dev/null", "/dev/random", "/dev/urandom"])),
        "(deny file-read-data\n  %s)" % sub(deny_read),  # explicit ops re-allow the Kimi home; file-read* would not
        "(allow file-read-data file-read-metadata\n  %s)" % sub(allow_read), ""])


def workspace(args, out, index, item):
    # The agent tree lives under the workspaces root; getcwd needs its ancestors readable, so the harness
    # output (receipts, logs, the profile) lives apart under `out` and is read-denied.
    sdir, wdir = out / "sessions" / ("%02d" % index), args.wroot / ("%02d" % index)
    ws, tmp, home, binp = wdir / "ws", wdir / "tmp", wdir / "home", wdir / "bin"
    for d in (ws / ".check/req", ws / ".check/resp", tmp, home, binp, sdir):
        d.mkdir(parents=True)
    shutil.copyfile(item / "program.c", ws / "program.c")
    executable(binp / "check", CHECK_CLIENT)
    execs = set()
    for tool in TOOLS + EXTRA_TOOLS:
        found = shutil.which(tool, path="/bin:/usr/bin:/usr/local/bin:/opt/homebrew/bin")
        if found: (binp / tool).symlink_to(found); execs.add(os.path.realpath(found))
        elif tool in TOOLS: raise repair.Failure("missing_tool_" + tool)
    execs.add(str(binp / "check")); execs.add(str(args.base / "login-shell"))
    siblings = [p for p in args.wroot.iterdir() if p.is_dir() and p != wdir and p != args.base]
    (sdir / "profile.sb").write_text(profile(execs, ws, tmp, home, binp, out, siblings))
    return dict(sdir=sdir, ws=ws, tmp=tmp, home=home, bin=binp, deny=[str(ROOT), "/etc", "/private/etc", str(out)] + [str(p) for p in siblings])


def environment(args, w):
    env = dict(HOME=str(w["home"]), KIMI_CODE_HOME=str(KHOME), PATH=str(w["bin"]), SHELL=str(args.base / "login-shell"),
               TMPDIR=str(w["tmp"]), TERM="dumb", NO_COLOR="1", KIMI_LOOP_MAX_STEPS_PER_TURN=str(args.max_steps),
               KIMI_CODE_NO_AUTO_UPDATE="1", KIMI_DISABLE_TELEMETRY="1", KIMI_CODE_BUILTIN_PRODUCT_SKILLS="0",
               KIMI_CODE_WATCH="0", KIMI_CODE_SEARCH_WORKER="0", KIMI_CODE_PERSISTENCE_MINIDB_READMODEL="0")  # the sandbox blocks fs watch
    env.update({n: os.environ[n] for n in ("USER", "LOGNAME", "LANG") if n in os.environ})
    if args.effort != "default": env["KIMI_MODEL_THINKING_EFFORT"] = args.effort
    return env


def container_build(stage, mode):
    if shutil.disk_usage(ROOT).free < 25 * 1024**3: raise repair.Failure("disk_below_25_GiB")
    name = "agent-repair-check-" + uuid.uuid4().hex
    try:
        result = repair.run([repair.CONTAINER, "run", "--name", name, "--platform", "linux/arm64", "--network", "none",
                             "--cpus", "1", "--memory", "512M", "--user", "65534:65534", "-i", "-v", str(stage) + ":/in:ro",
                             repair.IMAGE, "sh", "-c", CONTAINER_BUILD, "check", mode])
    finally:
        subprocess.run([repair.CONTAINER, "delete", "--force", name], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, env=repair.ENV, timeout=30)
    return result["stdout"] + (("\n" + result["stderr"]) if result["exit_code"] and result["stderr"] else "")


def evocc_run(src, stage):
    """The header of c3 and k: the evocc build, then the sealed assembly, link and run."""
    build = repair.run([EVOCC, "-S", "--target=aarch64-unknown-linux-gnu", "-O0", str(src), "-o", str(stage / "input.s")], cwd=stage)
    text = "=== evocc build exit %d\n%s" % (build["exit_code"], build["stderr"][:4000])
    text += container_build(stage, "c3") if build["exit_code"] == 0 else "=== evocc build failed; the container did not run\n"
    return text


def c3_diagnostics(src, stage):
    text = evocc_run(src, stage)
    check = repair.run([EVO, "check", "--json", str(src)], cwd=stage)
    try: codes = sorted({d["code"] for line in check["stdout"].splitlines() if line.strip() for d in json.loads(line).get("diagnostics", [])})
    except (ValueError, KeyError, TypeError, AttributeError): codes = []
    records = [check] + [repair.run([EVO, "explain", c, "--json"], cwd=stage) for c in codes] + [repair.run([EVO, "fix", str(src), "--plan", "--json"], cwd=stage)]
    return text + "=== evo diagnostics\n" + "".join(r["stdout"] if r["stdout"].strip() else r["stderr"] for r in records)


def k_diagnostics(src, stage):
    """Protocol B condition K: the c3 header and one concise-1 check record. It has no explain or plan record."""
    text = evocc_run(src, stage)
    check = repair.run([EVO, "check", "--json", "--form=concise-1", str(src)], cwd=stage)
    return text + MARK + (check["stdout"] if check["stdout"].strip() else check["stderr"])


def feedback(condition, src, stage):
    shutil.copyfile(src, stage / "input.c")
    if condition == "c3": return c3_diagnostics(stage / "input.c", stage)
    if condition == "k":
        return k_diagnostics(stage / "input.c", stage)
    return container_build(stage, condition)


def serve(condition, ws, handled, log):
    """Answer each pending check request. The file name is data; it must resolve inside the workspace."""
    req, resp = ws / ".check/req", ws / ".check/resp"
    for ready in sorted(req.glob("*.ready")):
        rid = ready.stem
        if rid in handled: continue
        handled.add(rid)
        name = (req / (rid + ".file")).read_text().strip() if (req / (rid + ".file")).is_file() else "program.c"
        wsr = ws.resolve(); target = (wsr / name).resolve()
        code, text, source = 0, "", None
        try:
            if not target.is_relative_to(wsr) or not target.is_file(): code, text = 2, "check: name a C file in the working directory\n"
            elif target.stat().st_size > 262144: code, text = 2, "check: the file is larger than 262144 bytes\n"
            else:
                source = digest(target)  # the log names the checked bytes; the served text does not change
                with repair.tempfile.TemporaryDirectory(prefix="agent-repair-check-") as scratch:
                    text = feedback(condition, target, Path(scratch))
        except (repair.Failure, repair.Stop, OSError, subprocess.SubprocessError) as error:
            code, text = 3, "check: internal error: %s\n" % type(error).__name__
        (resp / (rid + ".out")).write_text(text)
        (resp / (rid + ".code")).write_text(str(code))
        (resp / (rid + ".done")).write_text("")
        with open(log, "a") as handle: handle.write(json.dumps(dict(time=now(), file=name, exit=code, bytes=len(text), id=rid, sha256=source)) + "\n")


def refusal(record):
    text = json.dumps(record)
    return bool(re.search(r'"(?:status_code|statusCode)": (40[123]|429)\b', text) or STOP.search(text))


def overloaded(record):
    message = next((str(record[k]) for k in ("error_message", "errorMessage", "message") if record.get(k)), str(record.get("error", "")))
    return "429" in json.dumps(record) and "overloaded" in message.lower() and not re.search(r"\b(quota|limit|usage)\b", message, re.I)


def meta_stop(line):
    try: record = json.loads(line)
    except ValueError: return None
    if not isinstance(record, dict) or record.get("role") != "meta" or not re.search("retry|error", str(record.get("type"))): return None
    return json.dumps(record) if refusal(record) else None


def launch(args, w, env, condition):
    """Run one sandboxed session. Service check requests, watch for a refusal, and enforce the wall limit."""
    argv = ["/usr/bin/sandbox-exec", "-f", str(w["sdir"] / "profile.sb"), str(args.kimi), "-p", args.prompt_text,
            "--output-format", "stream-json", "-m", args.model, "--agent-file", str(args.base / "agent.md"), "--skills-dir", str(args.base / "skills")]
    status = stop = None; overloads = 0
    handled, checklog = set(), w["sdir"] / "checks.jsonl"
    with open(w["sdir"] / "stdout.jsonl", "wb") as out, open(w["sdir"] / "stderr.txt", "wb") as err, open(w["sdir"] / "stdout.jsonl", "rb") as reader:
        start, pending = time.monotonic(), b""
        proc = subprocess.Popen(argv, cwd=str(w["ws"]), env=env, stdin=subprocess.DEVNULL, stdout=out, stderr=err, start_new_session=True)
        try:
            while True:
                done = proc.poll() is not None
                serve(condition, w["ws"], handled, checklog)
                pending += reader.read()
                *lines, pending = pending.split(b"\n")
                for notice in filter(None, map(meta_stop, lines)):
                    if overloaded(json.loads(notice)):
                        overloads += 1
                        if overloads > 3: status, stop = "provider_overloaded", notice
                    else: status, stop = "stopped", notice; break
                if stop: status = status or "stopped"
                elif done: break
                elif time.monotonic() - start > args.wall: status = "timeout"
                elif os.fstat(out.fileno()).st_size + os.fstat(err.fileno()).st_size > CAPTURE: status = "capture_limit"
                if status: break
                time.sleep(0.3)
        finally:
            if proc.poll() is None: kill(proc)
    return status, stop, proc.returncode, round(time.monotonic() - start, 1)


def session_logs(w):
    key, paths = hashlib.sha256(str(w["ws"]).encode()).hexdigest()[:12], []
    for wire in sorted((KHOME / "sessions").glob("wd_*_%s/session_*/agents/*/wire.jsonl" % key)):
        target = w["sdir"] / "wire" / wire.parents[2].name / (wire.parent.name + ".jsonl")
        target.parent.mkdir(parents=True, exist_ok=True); shutil.copyfile(wire, target); paths.append(target)
    return paths


def parse(paths):
    m = dict(steps=0, request_kinds={}, models=[], efforts=[], tokens=dict.fromkeys(TOKENS, 0), tool_calls={}, calls=[], errors=[])
    results = {}
    for path in paths:
        for line in Path(path).read_text(errors="replace").splitlines():
            try: record = json.loads(line)
            except ValueError: continue
            if not isinstance(record, dict): continue
            if record.get("type") == "context.append_loop_event": record = record.get("event") or {}
            kind = record.get("type")
            if kind == "llm.request":
                m["steps"] += 1
                m["request_kinds"][str(record.get("kind"))] = m["request_kinds"].get(str(record.get("kind")), 0) + 1
                for field, key in (("models", "modelAlias"), ("efforts", "thinkingEffort")):
                    if record.get(key) not in m[field]: m[field].append(record.get(key))
            elif kind == "usage.record":
                for n in TOKENS: m["tokens"][n] += int((record.get("usage") or {}).get(n) or 0)
            elif kind == "tool.call":
                name, args = str(record.get("name")), record.get("args")
                m["tool_calls"][name] = m["tool_calls"].get(name, 0) + 1
                m["calls"].append((str(record.get("toolCallId")), name, args if isinstance(args, dict) else {"raw": args}))
            elif kind == "tool.result":
                r = record.get("result") or {}
                results[str(record.get("toolCallId"))] = (str(r.get("output", "")), bool(r.get("isError")))
            elif kind in ("turn.ended", "turn.step.retrying", "turn.step.interrupted") and (record.get("error") or record.get("errorName") or record.get("reason") == "error"):
                m["errors"].append({k: record.get(k) for k in ("type", "error", "errorName", "errorMessage", "statusCode", "message") if record.get(k) is not None})
    return m, results


def command_words(command):
    text = HEREDOC.sub(r"\1", command).replace("\n", ";")
    try: lexer = shlex.shlex(text, posix=True, punctuation_chars=True); lexer.whitespace_split = True; tokens = list(lexer)
    except ValueError: tokens = re.findall(r"[;|&()]+|[^\s;|&()]+", text)
    words, start = [], True
    for i, token in enumerate(tokens):
        if token in SEPARATORS: start = True
        elif start and not (re.fullmatch(r"\w+=.*", token) or token in PREFIXES):
            words.append(token); start = False
            if os.path.basename(token) in ("sh", "bash") and tokens[i + 1:i + 2] == ["-c"] and i + 2 < len(tokens): words += command_words(tokens[i + 2])
    return words


def path_tokens(command):
    toks = re.findall(r"[^\s'\"`;|&<>()=,]+", HEREDOC.sub(r"\1", command))
    return [t for t in toks if t.startswith(("~", "$HOME", "${HOME}", "$TMPDIR", "${TMPDIR}", "..")) or t.startswith("../") or "/../" in t or (t.startswith("/") and os.path.exists("/" + t[1:].split("/")[0]))]


def classify(token, w):
    """A path reference: 'scratch' (workspace or session temp), 'forbidden' (a sandbox-denied tree) or 'benign'."""
    if token.startswith(("$TMPDIR", "${TMPDIR}")): return "scratch"
    if token.startswith(("~", "$HOME", "${HOME}")): return "benign"
    p = os.path.normpath(os.path.join(str(w["ws"]), token))
    p = "/private" + p if p == "/tmp" or p.startswith("/tmp/") else p
    under = lambda base: p == base or p.startswith(base + os.sep)
    if under(str(w["ws"])) or under(str(w["tmp"])): return "scratch"
    if under(str(KHOME)): return "benign"
    if any(under(root) for root in w["deny"]) or (under(str(UHOME)) and not under(str(KHOME))): return "forbidden"
    return "benign"


def intents(name, args, w):
    """The web, tool or path references in one tool call: (kind, detail, class)."""
    found = []
    if name in WEB or name.startswith("mcp__"): found.append(("web", name, "forbidden"))
    elif name == "Bash":
        command = str(args.get("command", ""))
        for token in ([str(args["cwd"])] if args.get("cwd") else []) + path_tokens(command):
            found.append(("path", token, classify(token, w)))
        for word in command_words(command):
            if FORBIDDEN.match(os.path.basename(word)): found.append(("tool", word, "forbidden"))
    elif name in FILE_TOOLS:
        for key in ("path", "file_path") + (("pattern",) if name == "Glob" else ()):
            value = args.get(key)
            if isinstance(value, str) and (key != "pattern" or value.startswith(("/", "~", ".."))):
                found.append(("path", value, classify(value, w)))
    return found


def audit(calls, results, w):
    """Three classes: scratch use of the workspace or session temp, blocked attempts, successful escapes."""
    blocked, escapes, scratch = [], [], []
    for cid, name, args in calls:
        output, is_error = results.get(cid, ("", False))
        denied = bool(DENIED.search(output))
        for kind, detail, klass in intents(name, args, w):
            if klass == "benign": continue
            row = dict(tool=name, kind=kind, detail=detail[:200])
            if klass == "scratch": scratch.append(row)
            elif denied or is_error: blocked.append(row)
            else: escapes.append(row)
    return dict(blocked=blocked, escapes=escapes, scratch=scratch)


def repo_untracked():
    r = repair.run(["git", "status", "--porcelain", "--untracked-files=all"])
    return {line[3:] for line in r["stdout"].splitlines()} if r["exit_code"] == 0 else set()


def filesystem_escape(baseline):
    """A backstop for the sandbox: files the session added to the repository working tree. The sandbox
    confines writes to the workspace, the session temp and the Kimi home, so this stays empty."""
    return sorted(repo_untracked() - baseline)[:20]


def quota(kimi, port):
    env = dict(os.environ, KIMI_CODE_NO_AUTO_UPDATE="1", KIMI_DISABLE_TELEMETRY="1")
    server = subprocess.Popen([str(kimi), "web", "--no-open", "--port", str(port)], stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, env=env, start_new_session=True)
    base, result, token = "http://127.0.0.1:%d/api/v1/" % port, dict(time=now()), ""
    try:
        for _ in range(100):
            try: urllib.request.urlopen(base + "healthz", timeout=2).close(); break
            except OSError: time.sleep(0.3)
        token = (KHOME / "server.token").read_text().strip()
        request = urllib.request.Request(base + "oauth/usage", headers={"Authorization": "Bearer " + token})
        with urllib.request.urlopen(request, timeout=30) as response: result["data"] = json.loads(response.read()).get("data")
    except (OSError, ValueError) as error: result["error"] = type(error).__name__
    finally: kill(server)
    if token and token in json.dumps(result): result = dict(time=result["time"], error="token_in_response")
    return result


def ratios(q):
    u = ((q.get("data") or {}).get("quota") or {}).get("usages", {}) if isinstance(q.get("data"), dict) else {}
    return u.get("limit5h") or {}, u.get("limit7d") or {}


def pace(args, label, read=quota):
    """Read the plan quota. Wait out a nearly spent 5-hour window; stop when the 7-day ratio is reached."""
    q = read(args.kimi, args.quota_port); q["label"] = label
    five, week = ratios(q)
    if not isinstance(week.get("usedRatio"), (float, int)): return q, "quota_unavailable"
    if not hasattr(args, "initial_week_ratio"): args.initial_week_ratio = week["usedRatio"]
    args.max_week_ratio = min(args.max_week_ratio, args.initial_week_ratio + args.max_week_increase)
    if (week.get("usedRatio") or 0) >= args.max_week_ratio: return q, "stop_week"
    waited = 0.0
    while (five.get("usedRatio") or 0) >= 0.9 and waited < args.max_wait:
        reset = five.get("resetAt")
        try: seconds = (datetime.datetime.fromisoformat(reset.replace("Z", "+00:00")) - datetime.datetime.now(datetime.timezone.utc)).total_seconds()
        except (AttributeError, ValueError): seconds = 300
        seconds = max(5.0, min(seconds + 5, args.max_wait - waited))
        time.sleep(seconds); waited += seconds
        q = read(args.kimi, args.quota_port); q["label"] = label + "-waited"; five, week = ratios(q)
        if not isinstance(week.get("usedRatio"), (float, int)): return q, "quota_unavailable"
        if (week.get("usedRatio") or 0) >= args.max_week_ratio: return q, "stop_week"
    return q, "five_hour_quota" if (five.get("usedRatio") or 0) >= 0.9 else "ok"


def session(args, out, index, item, condition):
    w = workspace(args, out, index, item)
    original, repo_before = (item / "program.c").read_text(), repo_untracked()
    row = dict(index=index, item=item.name, condition=condition, model=args.model, effort=args.effort, workspace=str(w["ws"]), started_at=now())
    status, stop, code, seconds = launch(args, w, environment(args, w), condition)
    lines = (w["sdir"] / "stderr.txt").read_text(errors="replace").splitlines()
    fatal = [line for line in lines if line.startswith("error: failed to run prompt:")]
    final = (w["ws"] / "program.c").read_text(errors="replace") if (w["ws"] / "program.c").is_file() else ""
    logs = session_logs(w)
    m, results = parse(logs)
    stop = stop or next(filter(STOP.search, fatal), None) or next((json.dumps(e) for e in m["errors"] if refusal(e) and not overloaded(e)), None)
    if stop: status = status if status == "provider_overloaded" else "stopped"
    elif status is None: status = "completed" if code == 0 else "step_limit" if any("max_steps_exceeded" in line for line in fatal) else "error"
    classes = audit(m["calls"], results, w)
    classes["escapes"] += [dict(tool=None, kind="filesystem", detail=p) for p in filesystem_escape(repo_before)]
    checks = (w["sdir"] / "checks.jsonl").read_text().splitlines() if (w["sdir"] / "checks.jsonl").exists() else []
    bash = [str(a.get("command", "")) for _, n, a in m["calls"] if n == "Bash"]
    row.update(seconds=seconds, exit_status=code, status=status, stop=stop, fatal=fatal[:5] or (lines[-3:] if code else []),
               models_seen=m["models"], efforts_seen=m["efforts"], steps=m["steps"], request_kinds=m["request_kinds"],
               tokens=dict(m["tokens"], total=sum(m["tokens"].values())), tool_calls=m["tool_calls"],
               check_runs=sum(any(os.path.basename(x) == "check" for x in command_words(c)) for c in bash), check_served=len(checks),
               bash_commands=bash, audit={k: len(v) for k, v in classes.items()}, blocked=classes["blocked"][:40],
               escapes=classes["escapes"], scratch=classes["scratch"][:40], errors=m["errors"][:20],
               final_sha256=hashlib.sha256(final.encode()).hexdigest(), preprocessor=bool(re.search(r"#|%:|\?\?=", re.sub(r"\\\r?\n", "", final))))
    shutil.move(str(w["ws"]), str(w["sdir"] / "workspace"))
    if getattr(args, "order", "rotation") != "rotation":  # Protocol B: the scorer refuses below 25 GiB
        row["disk_wait"] = disk_wait("score %02d" % index)[0]
    try:
        row["score_builds"] = qualify_silent.score_builds(final, item)
        row["passed"] = status == "completed" and all(r["passed"] for r in row["score_builds"].values())
        metric, delta = repair.measure(original, final, json.loads((item / "fix-spans.json").read_text())["spans"])
        (w["sdir"] / "final.diff").write_text(delta)
        row.update(diff_size=metric["diff_added"] + metric["diff_removed"], span_hits=metric["spans_touched"],
                   all_spans=all(metric["spans_touched"]), changed_inside_spans=metric["changed_inside_spans"])
    except (repair.Failure, OSError, ValueError, KeyError) as error: row.update(passed=False, score_error=str(error))
    return row


def provenance(args):
    def text(*argv):
        try: return subprocess.run(argv, cwd=ROOT, capture_output=True, text=True, timeout=60).stdout.strip()
        except (OSError, subprocess.SubprocessError): return None
    try: image = json.loads(text(repair.CONTAINER, "image", "inspect", repair.IMAGE))[0]["configuration"]["descriptor"]["digest"]
    except (TypeError, ValueError, KeyError, IndexError): image = None
    return dict(commit=text("git", "rev-parse", "HEAD"), dirty=bool(text("git", "status", "--porcelain")), kimi=str(args.kimi),
                kimi_version=text(str(args.kimi), "--version"), kimi_sha256=digest(args.kimi), evo_sha256=digest(EVO), evocc_sha256=digest(EVOCC),
                image=repair.IMAGE, image_digest=image, harness_sha256=digest(HERE / "agent_repair.py"), repair_sha256=digest(HERE / "repair.py"),
                prompt_sha256=digest(args.prompt), scorer_sha256=digest(HERE / "qualify_silent.py"), sysroot=str(SYSROOT), python=sys.version.split()[0])


def totals(runs):
    total = dict(sessions=len(runs), passed=sum(r.get("passed") is True for r in runs), status={}, tool_calls={}, audit=dict(blocked=0, escapes=0, scratch=0))
    for key in ("steps", "seconds", "check_runs", "check_served"): total[key] = round(sum(r.get(key) or 0 for r in runs), 1)
    total["tokens"] = {k: sum(r.get("tokens", {}).get(k, 0) for r in runs) for k in TOKENS + ("total",)}
    for r in runs:
        total["status"][r["status"]] = total["status"].get(r["status"], 0) + 1
        for name, count in r.get("tool_calls", {}).items(): total["tool_calls"][name] = total["tool_calls"].get(name, 0) + count
        for k in total["audit"]: total["audit"][k] += r.get("audit", {}).get(k, 0)
    return total


def write(out, receipt): (out / "receipt.json").write_text(json.dumps(receipt, indent=1) + "\n")


def archive(out):
    with tarfile.open(out / "wire.tar.gz", "w:gz") as tar:
        for path in sorted(out.glob("sessions/*/wire/*/*.jsonl")):
            info = tar.gettarinfo(str(path), arcname=str(path.relative_to(out)))
            info.mtime, info.uid, info.gid, info.uname, info.gname = 0, 0, 0, "", ""
            with open(path, "rb") as handle: tar.addfile(info, handle)


def disk_wait(label):
    """Wait until 25 GiB are free: the check and the scorer refuse to run below that. Return the wait record,
    or None when no wait was needed, and whether the space is free now."""
    free, waited = shutil.disk_usage(ROOT).free, 0.0
    if free >= 25 * GIB:
        return None, True
    record = dict(label=label, started_at=now(), free_gib=[round(free / GIB, 2)])
    while free < 25 * GIB and waited < DISK_WAIT[1]:
        step = min(DISK_WAIT[0], DISK_WAIT[1] - waited)
        time.sleep(step)
        waited += step
        free = shutil.disk_usage(ROOT).free
        record["free_gib"].append(round(free / GIB, 2))
    record.update(ended_at=now(), seconds=waited, free=free >= 25 * GIB)
    return record, record["free"]


def binaries(kimi):
    """The SHA-256 of the frozen binaries. Protocol B stops when one changes."""
    return dict(evo_sha256=digest(EVO), evocc_sha256=digest(EVOCC), kimi_sha256=digest(kimi))


def protocol_b(args, names, rule):
    """Fix the Protocol B conditions, settings and items. With F_K, the rotation in run() runs F first at even
    zero-based indices and K first at odd ones, and each repetition reverses the one before it."""
    if args.condition and tuple(args.condition) != F_K:
        raise SystemExit("Protocol B runs --condition c3 --condition k, in this order")
    fixed = {k: SETTINGS_B[k] for k in ("model", "effort", "wall", "max_steps")}
    if {k: getattr(args, k) for k in fixed} != fixed or Path(args.prompt).resolve() != HERE / SETTINGS_B["prompt"]:
        raise SystemExit("Protocol B fixes the model, effort, wall, steps and prompt: %s" % json.dumps(SETTINGS_B))
    if args.max_week_ratio > SETTINGS_B["max_week_ratio"] or args.max_week_increase > SETTINGS_B["max_week_increase"]:
        raise SystemExit("Protocol B stops at the initial 7-day ratio plus 0.15, or at 0.85")
    if rule == "pilot-b":
        if names != sorted(PILOT_B) or args.reps != 1 or getattr(args, "first_repetition", 1) != 1:
            raise SystemExit("the Protocol B pilot runs its five items once: %s" % ", ".join(PILOT_B))
        names = list(PILOT_B)
    return list(F_K), names


def run(args):
    conditions, names, rule = args.condition or ["c1", "c3"], sorted(args.item), getattr(args, "order", "rotation")
    if rule != "rotation":
        conditions, names = protocol_b(args, names, rule)
    items = [args.items.resolve() / name for name in names]
    first = getattr(args, "first_repetition", 1)
    if first < 1 or args.reps < 1: raise SystemExit("repetition numbers must be positive")
    order = [(item, c, rep) for rep in range(first, first + args.reps) for i, item in enumerate(items)
             for c in conditions[(i + rep - 1) % len(conditions):] + conditions[:(i + rep - 1) % len(conditions)]]
    if len(order) > args.max_sessions: raise SystemExit("the plan has %d sessions; --max-sessions is %d" % (len(order), args.max_sessions))
    if args.out.resolve().is_relative_to(ROOT): raise SystemExit("--out must be outside the repository")
    if any(not (item / "program.c").is_file() or not (item / "fix-spans.json").is_file() for item in items): raise SystemExit("an item is incomplete")
    plan = dict(order=[dict(item=i.name, condition=c, repetition=r) for i, c, r in order], items=str(args.items), model=args.model, effort=args.effort,
                wall_seconds=args.wall, max_steps=args.max_steps, max_sessions=args.max_sessions, reps=args.reps, max_week_ratio=args.max_week_ratio)
    if rule != "rotation":
        plan["order_rule"] = rule
    if args.dry_run: print(json.dumps(plan)); return 0
    if not args.quota_port: raise SystemExit("--quota-port is required for a live run")
    frozen = binaries(args.kimi) if rule != "rotation" else None
    if rule != "rotation":  # Protocol B starts only after a passed precondition with the same binaries
        pre = json.loads(args.precondition.read_text()) if getattr(args, "precondition", None) else {}
        if not pre.get("passed") or any(pre.get("versions", {}).get(k) != v for k, v in frozen.items()):
            raise SystemExit("Protocol B needs --precondition: a passed record for these binaries")
    if not 0 < args.max_week_increase <= 0.30 or not 0 < args.max_week_ratio <= 0.85: raise SystemExit("quota limits exceed the approved bounds")
    plan["max_week_increase"] = args.max_week_increase
    plan["classes"] = {item.name: (item / "class").read_text().strip() for item in items}
    out, args.prompt_text = args.out.resolve(), args.prompt.read_text().strip()
    args.wroot = Path(repair.tempfile.mkdtemp(prefix="agent-repair-ws-")).resolve(); args.base = args.wroot / "env"
    out.mkdir(parents=True); (args.base / "skills").mkdir(parents=True)
    (args.base / "agent.md").write_text(AGENT_FILE); executable(args.base / "login-shell", LOGIN_SHELL)
    repair.ENV = dict(repair.ENV, EVOCC_SYSROOT=str(SYSROOT))
    receipt = dict(schema="agent-repair.v4", phase=args.phase, date=now(), plan=plan, provenance=provenance(args), mechanism="seatbelt",
                   workspaces=str(args.wroot), prompt=args.prompt_text, quota=[], runs=[], totals=None, stop_reason=None)
    if rule != "rotation":
        receipt.update(binaries=frozen, disk_waits=[])
    write(out, receipt)
    retried = {}
    for index, (item, condition, repetition) in enumerate(order, 1):
        if rule != "rotation":
            wait, free = disk_wait("before %02d" % index)
            receipt["disk_waits"] += [wait] if wait else []
            receipt["stop_reason"] = None if free else "disk_below_25_GiB"
            if free and binaries(args.kimi) != frozen:
                receipt["stop_reason"] = "binaries_changed"
            if receipt["stop_reason"] or wait:
                write(out, receipt)
            if receipt["stop_reason"]:
                break
        if args.quota_port:
            q, decision = pace(args, "before %02d" % index); receipt["quota"].append(q)
            if decision != "ok": receipt["stop_reason"] = decision; write(out, receipt); break
        try: row = session(args, out, index, item, condition)
        except Exception as error: row = dict(index=index, item=item.name, condition=condition, status="harness_error", error="%s: %s" % (type(error).__name__, error)); receipt["stop_reason"] = "harness_error"
        row["repetition"] = repetition
        row["retry_of"] = retried.get((item, condition, repetition))
        if row["status"] == "provider_overloaded" and (item, condition, repetition) not in retried:
            retried[item, condition, repetition] = index; order.append((item, condition, repetition))
        receipt["runs"].append(row); receipt["totals"] = totals(receipt["runs"]); write(out, receipt)
        if row["status"] == "stopped": receipt["stop_reason"] = receipt["stop_reason"] or "provider_refusal"
        if row.get("escapes"): receipt["stop_reason"] = receipt["stop_reason"] or "escape_detected"
        if receipt["stop_reason"]: break
    if rule != "rotation" and receipt["stop_reason"] is None and binaries(args.kimi) != frozen:
        receipt["stop_reason"] = "binaries_changed"  # a change during the last session
    # Protocol B reads the quota at the end of a stopped run too, unless a quota read itself stopped it.
    paced = receipt["stop_reason"] in ("stop_week", "quota_unavailable", "five_hour_quota")
    if args.quota_port and (receipt["stop_reason"] is None or (rule != "rotation" and not paced)): receipt["quota"].append(pace(args, "final")[0])
    receipt.update(stop_reason=receipt["stop_reason"] or "completed", unattempted=len(order) - len(receipt["runs"]), totals=totals(receipt["runs"]))
    archive(out); write(out, receipt)
    print(json.dumps(dict(runs=len(receipt["runs"]), stop_reason=receipt["stop_reason"], totals=receipt["totals"])))
    return 0 if receipt["stop_reason"] == "completed" else 1


def split(text):
    """The header and the EvoCC part of a c3 or k check result. The EvoCC part is None without the marker."""
    head, mark, part = text.partition(MARK)
    return (head, part) if mark else (text, None)


def scratch_of(text, prefix):
    """The scratch directory that a check result names, or None. Every path in it ends with /input.c."""
    found = re.search(r'(/[^"\s]*/%s[^/"\s]+)/input\.c' % re.escape(prefix), text)
    return found[1] if found else None


def current_scope(text):
    """The frozen EvoCC part with only the T03s scope correction, as crates/evocc/tests/frozen_feedback.rs applies
    it: the scope of each completed record changes from OLD_SCOPE to NEW_SCOPE. Other bytes are unchanged."""
    if text is None:
        return None
    field, lines = '"scope":' + json.dumps(OLD_SCOPE), []
    for line in text.splitlines(keepends=True):
        try:
            record = json.loads(line)
        except ValueError:
            lines.append(line)
            continue
        completion = (record.get("completion") or record) if isinstance(record, dict) else {}
        if isinstance(completion, dict) and completion.get("status") == "completed" and completion.get("scope") == OLD_SCOPE:
            if line.count(field) != 1:
                raise ValueError("the completed record does not hold exactly one old scope field")
            line = line.replace(field, '"scope":' + json.dumps(NEW_SCOPE))
        lines.append(line)
    return "".join(lines)


def concise_strings(path):
    """The recorded concise-1 strings by (item, source), with the scratch directory written as <scratch>."""
    record = json.loads(Path(path).read_text())
    return {(i["item"], s): e["concise-1"].replace(e["scratch"], "<scratch>") for i in record["items"] for s, e in i["feedback"].items()}


def versions(kimi):
    """The compiler and agent identity. The Kimi CLI runs without an update check."""
    def text(*argv):
        env = dict(os.environ, KIMI_CODE_NO_AUTO_UPDATE="1", KIMI_DISABLE_TELEMETRY="1")
        try:
            return subprocess.run([str(x) for x in argv], capture_output=True, text=True, timeout=60, env=env).stdout
        except (OSError, subprocess.SubprocessError):
            return None
    return dict(evo_version=text(EVO, "version"), evocc_version=text(EVOCC, "--version"), evo_sha256=digest(EVO),
                evocc_sha256=digest(EVOCC), kimi_version=(text(kimi, "--version") or "").strip() or None, kimi_sha256=digest(kimi))


def precondition(args):
    """Protocol B precondition, without a model request. For the 24 items and both sources, the EvoCC part of the
    F result must equal the frozen C3 string after scratch-path replacement and the T03s scope correction, and the
    K record must equal the recorded concise-1 string with the same correction. The K header must equal the frozen
    header."""
    frozen = json.loads(args.qualification.read_text())["items"]
    concise, rows, waits = concise_strings(args.concise), [], []
    repair.ENV = dict(repair.ENV, EVOCC_SYSROOT=str(SYSROOT))
    before = versions(args.kimi)
    for entry in frozen:
        for source in ("program.c", "fixed.c"):
            want = entry["feedback"][source]["c3"]
            recorded, (want_head, want_part) = scratch_of(want, "silent-feedback-"), split(want)
            got = {}
            for condition in F_K:
                wait, free = disk_wait("%s %s %s" % (entry["item"], source, condition))
                waits += [wait] if wait else []
                if not free:
                    raise SystemExit("disk below 25 GiB for 6 hours; the precondition did not finish")
                with repair.tempfile.TemporaryDirectory(prefix="silent-feedback-") as tmp:
                    stage = Path(tmp)
                    stage.chmod(0o755)  # as qualify_silent.qualify, which wrote the frozen strings
                    text = feedback(condition, args.items / entry["item"] / source, stage)
                    got[condition] = (text.replace(str(stage), recorded), text.replace(str(stage), "<scratch>"))
            f_text, (k_head, k_part) = got["c3"][0], split(got["k"][1])
            rows.append(dict(item=entry["item"], source=source, full_1_bytes=len((split(f_text)[1] or "").encode()),
                             full_1_identical=split(f_text)[1] == want_part, c3_identical=f_text == want,
                             full_1_identical_after_scope=split(f_text)[1] == current_scope(want_part),
                             concise_1_bytes=len((k_part or "").encode()), concise_1_sha256=hashlib.sha256((k_part or "").encode()).hexdigest(),
                             concise_1_identical=k_part == concise[entry["item"], source],
                             concise_1_identical_after_scope=k_part == current_scope(concise[entry["item"], source]),
                             k_header_identical=k_head == want_head,
                             concise_1_form=(k_part or "").startswith('{"form":"concise-1"')))
    after = versions(args.kimi)
    summary = {k: sum(r[k] for r in rows) for k in ("full_1_identical", "full_1_identical_after_scope", "c3_identical",
                                                    "concise_1_identical", "concise_1_identical_after_scope",
                                                    "k_header_identical", "concise_1_form")}
    record = dict(schema="agent-repair.p22-precondition.v1", date=now(), records_expected=48, summary=summary, versions=before,
                  versions_unchanged=before == after, qualification=dict(path=str(args.qualification), sha256=digest(args.qualification)),
                  concise=dict(path=str(args.concise), sha256=digest(args.concise)), image=repair.IMAGE, disk_waits=waits,
                  method="Each source runs once per condition in a fresh silent-feedback- scratch directory, as qualify_silent.qualify "
                         "did. F replaces the scratch path with the recorded one. K writes it as <scratch>, as the fixture does. "
                         "Both comparisons allow only the T03s scope correction (current_scope); the raw counts are kept.",
                  scope_correction=dict(old=OLD_SCOPE, new=NEW_SCOPE),
                  passed=len(rows) == 48 and summary["full_1_identical_after_scope"] == 48
                  and summary["concise_1_identical_after_scope"] == 48 and before == after, records=rows)
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(record, indent=1) + "\n")
    print(json.dumps(dict(passed=record["passed"], summary=summary)))
    return 0 if record["passed"] else 1


def served(out, row):
    """The check results of one session in serving order, as (log record, served text or None)."""
    sdir, rows = out / "sessions" / ("%02d" % row["index"]), []
    log, resp = sdir / "checks.jsonl", sdir / "workspace/.check/resp"
    for line in log.read_text().splitlines() if log.is_file() else []:
        record = json.loads(line)
        path = resp / ("%s.out" % record.get("id"))
        rows.append((record, path.read_text() if path.is_file() else None))
    return rows


def go(args):
    """Evaluate the go criteria of the Protocol B pilot from a finished run. Write pilot-receipt.json beside it."""
    out = args.out.resolve()
    receipt, pre = json.loads((out / "receipt.json").read_text()), json.loads(args.precondition.read_text())
    frozen = {e["item"]: e["feedback"] for e in json.loads(args.qualification.read_text())["items"]}
    concise, n = concise_strings(args.concise), dict.fromkeys(("k", "k_evocc", "k_form", "k_known", "k_equal", "f", "f_known", "f_equal", "missing"), 0)
    for row in receipt["runs"]:
        known, key = {digest(args.items / row["item"] / s): s for s in ("program.c", "fixed.c")}, "k" if row["condition"] == "k" else "f"
        for record, text in served(out, row):
            n[key] += 1
            source, live = known.get(record.get("sha256")), scratch_of(text or "", "agent-repair-check-")
            if text is None:
                n["missing"] += 1
            elif key == "k":
                part = split(text.replace(live, "<scratch>") if live else text)[1]
                if part and part.strip():
                    n["k_evocc"] += 1
                    n["k_form"] += part.startswith('{"form":"concise-1"')
                if source:
                    n["k_known"] += 1
                    n["k_equal"] += part == current_scope(concise[row["item"], source])
            elif source:  # F: an unchanged corpus source must give the frozen bytes
                want = frozen[row["item"]][source]["c3"]
                part = split(text.replace(live, scratch_of(want, "silent-feedback-")) if live else text)[1]
                n["f_known"] += 1
                n["f_equal"] += part == current_scope(split(want)[1])
    planned = [(p["item"], p["condition"], p["repetition"]) for p in receipt["plan"]["order"]]
    cells = {(r["item"], r["condition"], r.get("repetition")): r for r in receipt["runs"]}  # a retry replaces its cell
    final = [cells[c] for c in planned if c in cells]
    reads = receipt["quota"]
    week = [ratios(q)[1].get("usedRatio") for q in reads]
    escapes, blocked = (sum(r.get("audit", {}).get(k, 0) for r in receipt["runs"]) for k in ("escapes", "blocked"))
    recorded = all(isinstance(r.get("steps"), int) and r.get("tokens", {}).get("total") and len(r.get("score_builds") or {}) == 3
                   and "score_error" not in r for r in receipt["runs"])
    unknown = [r["index"] for r in final if r["status"] not in ("completed", "timeout", "step_limit") or "score_error" in r or r.get("escapes")]
    held = [escapes == 0 and receipt["stop_reason"] != "escape_detected",
            n["k_evocc"] > 0 and n["k_form"] == n["k_evocc"] and not n["missing"],
            n["f_known"] > 0 and n["f_equal"] == n["f_known"] and not n["missing"],
            len(reads) == len(receipt["runs"]) + 1 and all(isinstance(w, (int, float)) for w in week) and recorded,
            not unknown and len(final) == len(planned)]
    evidence = ["%d escapes and %d blocked attempts in %d sessions" % (escapes, blocked, len(receipt["runs"])),
                "%d of %d K results with EvoCC output start so; %d K results; %d served texts missing" % (n["k_form"], n["k_evocc"], n["k"], n["missing"]),
                "%d of %d F results for an unchanged corpus source are equal; %d F results" % (n["f_equal"], n["f_known"], n["f"]),
                "%d of %d quota reads succeed for %d sessions; steps, tokens and three scores recorded: %s"
                % (sum(isinstance(w, (int, float)) for w in week), len(reads), len(receipt["runs"]), recorded),
                "%d of %d planned sessions have a final attempt; unknown outcomes in sessions %s" % (len(final), len(planned), unknown)]
    criteria = list(zip(GO_B, held, evidence))
    after = versions(args.kimi)
    numbers = [w for w in week if isinstance(w, (int, float))]
    pilot = dict(receipt, protocol=dict(path="tools/experiment/protocol-7.json", sha256=digest(HERE / "protocol-7.json"),
                                        status=json.loads((HERE / "protocol-7.json").read_text())["status"]),
                 precondition=dict(path=str(args.precondition), sha256=digest(args.precondition), passed=pre["passed"], summary=pre["summary"]),
                 go=dict(criteria=[dict(criterion=c, held=h, evidence=e) for c, h, e in criteria], held=all(h for _, h, _ in criteria), checks=n),
                 quota_change=dict(first=numbers[0] if numbers else None, last=numbers[-1] if numbers else None,
                                   increase=round(numbers[-1] - numbers[0], 6) if numbers else None, reads=len(reads)),
                 provenance_after=after, binaries_unchanged=all(after[k] == pre["versions"][k] for k in after),
                 analysis="Pilot data stay out of the measured analysis. The five items stay in the population.")
    (out / "pilot-receipt.json").write_text(json.dumps(pilot, indent=1) + "\n")
    print(json.dumps(dict(held=pilot["go"]["held"], criteria=[h for _, h, _ in criteria], quota=pilot["quota_change"])))
    return 0 if pilot["go"]["held"] else 1


def main(argv=None):
    top = argparse.ArgumentParser(description=__doc__)
    commands = top.add_subparsers(dest="command", required=True)
    parser = commands.add_parser("run", help="run the planned sessions")
    parser.add_argument("--items", type=Path, required=True); parser.add_argument("--item", action="append", required=True)
    parser.add_argument("--condition", action="append", choices=("c1", "c3", "k")); parser.add_argument("--reps", type=int, default=1)
    parser.add_argument("--first-repetition", type=int, default=1)
    parser.add_argument("--max-sessions", type=int, default=9); parser.add_argument("--phase", default="pilot")
    parser.add_argument("--out", type=Path, required=True); parser.add_argument("--kimi", type=Path, default=KIMI)
    parser.add_argument("--model", default="kimi-code/k3"); parser.add_argument("--effort", default="low", help="KIMI_MODEL_THINKING_EFFORT, or 'default'")
    parser.add_argument("--wall", type=float, default=900); parser.add_argument("--max-steps", type=int, default=100)
    parser.add_argument("--prompt", type=Path, default=HERE / "prompt-3.txt"); parser.add_argument("--quota-port", type=int)
    parser.add_argument("--max-week-ratio", type=float, required=True, help="stop before a session when the 7-day plan ratio is at or above this")
    parser.add_argument("--max-week-increase", type=float, default=0.30)
    parser.add_argument("--max-wait", type=float, default=21600.0); parser.add_argument("--dry-run", action="store_true", help="print the plan; start no session")
    parser.add_argument("--order", choices=("rotation", "protocol-b", "pilot-b"), default="rotation",
                        help="protocol-b and pilot-b fix the conditions c3 (F) and k (K) and the Protocol B settings")
    parser.add_argument("--precondition", type=Path, help="a passed precondition record; Protocol B needs one for these binaries")
    corpus = ROOT / "tests/fixtures/agent-repair-4"
    checks = {}
    for name, text in (("precondition", "compare the F and K feedback of the 48 corpus sources with the frozen strings"),
                       ("go", "evaluate the Protocol B pilot go criteria of a finished run")):
        checks[name] = commands.add_parser(name, help=text)
        checks[name].add_argument("--items", type=Path, default=corpus)
        checks[name].add_argument("--qualification", type=Path, default=corpus / "qualification.json")
        checks[name].add_argument("--concise", type=Path, default=corpus / "concise-1-feedback.json")
        checks[name].add_argument("--kimi", type=Path, default=KIMI)
        checks[name].add_argument("--out", type=Path, required=True, help="the record to write, or the run directory for go")
    checks["go"].add_argument("--precondition", type=Path, required=True)
    args = top.parse_args(argv)
    return dict(run=run, precondition=precondition, go=go)[args.command](args)


if __name__ == "__main__":
    raise SystemExit(main())
