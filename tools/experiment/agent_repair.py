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
    name = "agent-repair-check-" + uuid.uuid4().hex
    try:
        result = repair.run([repair.CONTAINER, "run", "--name", name, "--platform", "linux/arm64", "--network", "none",
                             "--cpus", "1", "--memory", "512M", "--user", "65534:65534", "-i", "-v", str(stage) + ":/in:ro",
                             repair.IMAGE, "sh", "-c", CONTAINER_BUILD, "check", mode])
    finally:
        subprocess.run([repair.CONTAINER, "delete", "--force", name], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, env=repair.ENV, timeout=30)
    return result["stdout"] + (("\n" + result["stderr"]) if result["exit_code"] and result["stderr"] else "")


def c3_diagnostics(src, stage):
    build = repair.run([EVOCC, "-S", "--target=aarch64-unknown-linux-gnu", "-O0", str(src), "-o", str(stage / "input.s")], cwd=stage)
    text = "=== evocc build exit %d\n%s" % (build["exit_code"], build["stderr"][:4000])
    text += container_build(stage, "c3") if build["exit_code"] == 0 else "=== evocc build failed; the container did not run\n"
    check = repair.run([EVO, "check", "--json", str(src)], cwd=stage)
    try: codes = sorted({d["code"] for line in check["stdout"].splitlines() if line.strip() for d in json.loads(line).get("diagnostics", [])})
    except (ValueError, KeyError, TypeError, AttributeError): codes = []
    records = [check] + [repair.run([EVO, "explain", c, "--json"], cwd=stage) for c in codes] + [repair.run([EVO, "fix", str(src), "--plan", "--json"], cwd=stage)]
    return text + "=== evo diagnostics\n" + "".join(r["stdout"] if r["stdout"].strip() else r["stderr"] for r in records)


def feedback(condition, src, stage):
    shutil.copyfile(src, stage / "input.c")
    if condition == "c3": return c3_diagnostics(stage / "input.c", stage)
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
        code, text = 0, ""
        try:
            if not target.is_relative_to(wsr) or not target.is_file(): code, text = 2, "check: name a C file in the working directory\n"
            elif target.stat().st_size > 262144: code, text = 2, "check: the file is larger than 262144 bytes\n"
            else:
                with repair.tempfile.TemporaryDirectory(prefix="agent-repair-check-") as scratch:
                    text = feedback(condition, target, Path(scratch))
        except (repair.Failure, repair.Stop, OSError, subprocess.SubprocessError) as error:
            code, text = 3, "check: internal error: %s\n" % type(error).__name__
        (resp / (rid + ".out")).write_text(text)
        (resp / (rid + ".code")).write_text(str(code))
        (resp / (rid + ".done")).write_text("")
        with open(log, "a") as handle: handle.write(json.dumps(dict(time=now(), file=name, exit=code, bytes=len(text))) + "\n")


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
    try:
        row["score_builds"] = qualify_silent.score_builds(final, item)
        row["passed"] = all(r["passed"] for r in row["score_builds"].values())
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


def run(args):
    conditions = args.condition or ["c1", "c3"]
    items = [args.items.resolve() / name for name in args.item]
    order = [(item, c) for rep in range(args.reps) for i, item in enumerate(items)
             for c in conditions[(i + rep) % len(conditions):] + conditions[:(i + rep) % len(conditions)]]
    if len(order) > args.max_sessions: raise SystemExit("the plan has %d sessions; --max-sessions is %d" % (len(order), args.max_sessions))
    if args.out.resolve().is_relative_to(ROOT): raise SystemExit("--out must be outside the repository")
    if any(not (item / "program.c").is_file() or not (item / "fix-spans.json").is_file() for item in items): raise SystemExit("an item is incomplete")
    plan = dict(order=[dict(item=i.name, condition=c) for i, c in order], items=str(args.items), model=args.model, effort=args.effort,
                wall_seconds=args.wall, max_steps=args.max_steps, max_sessions=args.max_sessions, reps=args.reps, max_week_ratio=args.max_week_ratio)
    if args.dry_run: print(json.dumps(plan)); return 0
    if not args.quota_port: raise SystemExit("--quota-port is required for a live run")
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
    write(out, receipt)
    retried = {}
    for index, (item, condition) in enumerate(order, 1):
        if args.quota_port:
            q, decision = pace(args, "before %02d" % index); receipt["quota"].append(q)
            if decision != "ok": receipt["stop_reason"] = decision; write(out, receipt); break
        try: row = session(args, out, index, item, condition)
        except Exception as error: row = dict(index=index, item=item.name, condition=condition, status="harness_error", error="%s: %s" % (type(error).__name__, error)); receipt["stop_reason"] = "harness_error"
        row["retry_of"] = retried.get((item, condition))
        if row["status"] == "provider_overloaded" and (item, condition) not in retried:
            retried[item, condition] = index; order.append((item, condition))
        receipt["runs"].append(row); receipt["totals"] = totals(receipt["runs"]); write(out, receipt)
        if row["status"] == "stopped": receipt["stop_reason"] = receipt["stop_reason"] or "provider_refusal"
        if row.get("escapes"): receipt["stop_reason"] = receipt["stop_reason"] or "escape_detected"
        if receipt["stop_reason"]: break
    if args.quota_port and receipt["stop_reason"] is None: receipt["quota"].append(pace(args, "final")[0])
    receipt.update(stop_reason=receipt["stop_reason"] or "completed", unattempted=len(order) - len(receipt["runs"]), totals=totals(receipt["runs"]))
    archive(out); write(out, receipt)
    print(json.dumps(dict(runs=len(receipt["runs"]), stop_reason=receipt["stop_reason"], totals=receipt["totals"])))
    return 0 if receipt["stop_reason"] == "completed" else 1


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("run",))
    parser.add_argument("--items", type=Path, required=True); parser.add_argument("--item", action="append", required=True)
    parser.add_argument("--condition", action="append", choices=("c1", "c3")); parser.add_argument("--reps", type=int, default=1)
    parser.add_argument("--max-sessions", type=int, default=9); parser.add_argument("--phase", default="pilot")
    parser.add_argument("--out", type=Path, required=True); parser.add_argument("--kimi", type=Path, default=KIMI)
    parser.add_argument("--model", default="kimi-code/k3"); parser.add_argument("--effort", default="low", help="KIMI_MODEL_THINKING_EFFORT, or 'default'")
    parser.add_argument("--wall", type=float, default=900); parser.add_argument("--max-steps", type=int, default=100)
    parser.add_argument("--prompt", type=Path, default=HERE / "prompt-3.txt"); parser.add_argument("--quota-port", type=int)
    parser.add_argument("--max-week-ratio", type=float, required=True, help="stop before a session when the 7-day plan ratio is at or above this")
    parser.add_argument("--max-week-increase", type=float, default=0.30)
    parser.add_argument("--max-wait", type=float, default=21600.0); parser.add_argument("--dry-run", action="store_true", help="print the plan; start no session")
    return run(parser.parse_args(argv))


if __name__ == "__main__":
    raise SystemExit(main())
