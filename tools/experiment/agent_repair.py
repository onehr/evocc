#!/usr/bin/env python3
"""Third repair experiment: the Kimi Code CLI repairs one C program per session (one item and condition).
The agent works in a fresh directory with only program.c and `check`; only `check` differs between conditions.
The harness scores the final file with the unchanged hidden test of repair.py. Standard library; Python 3.9+."""
import argparse, datetime, hashlib, json, os, re, shlex, shutil, signal, subprocess, sys, tarfile, time, urllib.request, uuid
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import repair  # noqa: E402  (scoring, diff measure and sealed container of the second experiment)

ROOT, KIMI = repair.ROOT, Path.home() / ".kimi-code/bin/kimi"
TOOLS = "sh bash cat ls head tail sed awk grep diff wc sort uniq cut tr echo printf cp mv rm mkdir test true false".split()
EXTRA_TOOLS = ["rg"]  # Kimi's Grep and Glob tools use rg from PATH; without it Kimi downloads rg into its home.
TOKENS = ("inputOther", "inputCacheRead", "inputCacheCreation", "output")
C0_TEXT = "No feedback is available in this environment.\n"
# The agent file removes the web tools from the main agent. Coder sub-agents keep them, so delegation is off too.
AGENT_FILE = ("---\nname: evocc-repair\ndescription: The default Kimi Code agent without web tools and without sub-agents.\n"
              "disallowedTools: WebSearch, FetchURL, Agent, AgentSwarm\n---\n${base_prompt}\n")
# Kimi runs "$SHELL -l -c /usr/bin/env" at start and appends that PATH. This stand-in reads no profile.
LOGIN_SHELL = "#!/bin/sh\nexec /usr/bin/env\n"
WEB, FILE_TOOLS = {"WebSearch", "FetchURL", "WebFetch"}, {"Read", "Write", "Edit", "Grep", "Glob", "ReadMediaFile"}
COMPILER = re.compile(r"(cc|c89|c99|gcc|g\+\+|c\+\+|clang|clang\+\+|tcc|icc|icx|zig|xcrun|cc1|evo|evocc)(-[0-9.]+)?")
STOP = re.compile(r"(?i)rate.?limit|quota|balance|auth|unauthori|forbidden|login|provider\.\w+: (40[123]|429)\b")
HEREDOC = re.compile(r"(<<-?[ \t]*(['\"]?)(\w+)\2[^\n]*)\n(?:.*?\n)??[ \t]*\3[ \t]*(?=\n|$)", re.S)
SEPARATORS = {";", "&&", "||", "|", "&", "(", ")", ";;", "|&"}
PREFIXES = {"env", "command", "exec", "time", "nice", "nohup", "sudo", "xargs", "builtin", "then", "do", "else", "if", "while", "until", "!"}
CAPTURE = 64 * 1024 ** 2

def now(): return datetime.datetime.now(datetime.timezone.utc).isoformat(timespec="seconds")
def digest(path): return hashlib.sha256(Path(path).read_bytes()).hexdigest()

def executable(path, text): path.write_text(text); path.chmod(0o755)

def check(condition, work, log, names):
    """The `check` command of one workspace. The generated script calls this in a new process."""
    signal.signal(signal.SIGTERM, lambda *_: sys.exit(143))  # finally blocks then delete the container
    work, code = Path(work), 0
    path = (Path.cwd() / (names[0] if names else "program.c")).resolve()
    if condition == "c0": text = C0_TEXT
    elif not path.is_relative_to(work) or not path.is_file(): text, code = "check: give a C file in the working directory\n", 2
    elif path.stat().st_size > 65536: text, code = "check: the file is larger than 65536 bytes\n", 2
    else:
        try: text = c1_feedback(path.read_text(errors="replace")) if condition == "c1" else c3_feedback(work, path.relative_to(work))
        except (repair.Failure, OSError) as error: text, code = "check: internal error: %s\n" % error, 3
    with open(log, "a") as handle: handle.write(json.dumps(dict(time=now(), cwd=os.getcwd(), args=names, exit_code=code, output=text)) + "\n")
    sys.stdout.write(text)
    return code

def c1_feedback(source):
    """Sealed gcc-14 UBSan build and run, as repair.py C1: compiler stderr, program stderr, exit codes."""
    lines = []
    for step in repair.native("c1", source):
        lines += ["$ " + " ".join(step["argv"])] + ([step["stderr"].rstrip("\n")] if step["stderr"] else []) + ["exit status %d" % step["exit_code"]]
    return "\n".join(lines) + "\n"

def c3_feedback(work, name):
    """Host evo check, explain for each code and fix --plan, as repair.py C3, on the workspace file."""
    def evo(*argv): return repair.run([repair.EVO, *argv], cwd=work)
    records = [evo("check", "--json", name)]
    try: codes = sorted({d["code"] for line in records[0]["stdout"].splitlines() if line.strip() for d in json.loads(line).get("diagnostics", [])})
    except (ValueError, KeyError, TypeError, AttributeError): codes = []
    records += [evo("explain", code, "--json") for code in codes] + [evo("fix", name, "--plan", "--json")]
    return "".join(r["stdout"] if r["stdout"].strip() else r["stderr"] for r in records)

def workspace(args, out, index, item, condition):
    sdir, work = out / "sessions" / ("%02d" % index), (args.workspaces / ("w%02d-%s" % (index, uuid.uuid4().hex[:8]))).resolve()
    (sdir / "bin").mkdir(parents=True); work.mkdir(parents=True)
    shutil.copyfile(item / "program.c", work / "program.c")
    words = map(shlex.quote, (sys.executable, str(HERE / "agent_repair.py"), condition, str(work), str(sdir / "checks.jsonl")))
    executable(sdir / "check", "#!/bin/sh\nexec %s -I %s check %s %s %s \"$@\"\n" % tuple(words))
    executable(work / "check", "#!/bin/sh\nexec %s \"$@\"\n" % shlex.quote(str(sdir / "check")))
    for tool in TOOLS + EXTRA_TOOLS:
        found = shutil.which(tool, path="/bin:/usr/bin:/usr/local/bin:/opt/homebrew/bin")
        if found: (sdir / "bin" / tool).symlink_to(found)
        elif tool in TOOLS: raise repair.Failure("missing_tool_" + tool)
    (sdir / "bin" / "check").symlink_to(work / "check")
    return sdir, work

def environment(args, base, sdir):
    env = dict(HOME=os.environ["HOME"], PATH=str(sdir / "bin"), SHELL=str(base / "login-shell"), TERM="dumb", NO_COLOR="1",
               KIMI_LOOP_MAX_STEPS_PER_TURN=str(args.max_steps), KIMI_CODE_NO_AUTO_UPDATE="1", KIMI_DISABLE_TELEMETRY="1",
               KIMI_CODE_BUILTIN_PRODUCT_SKILLS="0")
    env.update({name: os.environ[name] for name in ("USER", "LOGNAME", "LANG", "TMPDIR") if name in os.environ})
    if args.effort != "default": env["KIMI_MODEL_THINKING_EFFORT"] = args.effort
    return env

def stop_process(process):
    try: os.killpg(process.pid, signal.SIGTERM); process.wait(10)
    except subprocess.TimeoutExpired: os.killpg(process.pid, signal.SIGKILL); process.wait()
    except ProcessLookupError: process.wait()

def refusal(record):
    """A rate-limit, quota or authentication error, by HTTP status field or by the error words of the CLI."""
    text = json.dumps(record)
    return bool(re.search(r'"(?:status_code|statusCode)": (40[123]|429)\b', text) or STOP.search(text))

def meta_stop(line):
    """A stream-json retry or error notice that reports a refusal; numbers in tool output never count."""
    try: record = json.loads(line)
    except ValueError: return None
    if not isinstance(record, dict) or record.get("role") != "meta" or not re.search("retry|error", str(record.get("type"))): return None
    return json.dumps(record) if refusal(record) else None

def launch(argv, work, env, sdir, wall):
    """Run one session. Kill the process group at the wall limit or at the first refusal notice."""
    status = stop = None
    with open(sdir / "stdout.jsonl", "wb") as out, open(sdir / "stderr.txt", "wb") as err, open(sdir / "stdout.jsonl", "rb") as reader:
        start, pending = time.monotonic(), b""
        process = subprocess.Popen(argv, cwd=work, env=env, stdin=subprocess.DEVNULL, stdout=out, stderr=err, start_new_session=True)
        try:
            while True:
                done = process.poll() is not None
                pending += reader.read()
                *lines, pending = pending.split(b"\n")
                stop = stop or next(filter(None, map(meta_stop, lines)), None)
                if stop: status = "stopped"
                elif done: break
                elif time.monotonic() - start > wall: status = "timeout"
                elif os.fstat(out.fileno()).st_size + os.fstat(err.fileno()).st_size > CAPTURE: status = "capture_limit"
                if status: break
                time.sleep(0.5)
        finally:
            if process.poll() is None: stop_process(process)
    return status, stop, process.returncode, time.monotonic() - start

def session_logs(work, sdir):
    """Copy every wire.jsonl of the Kimi session whose working directory is this workspace."""
    key, paths = hashlib.sha256(str(work).encode()).hexdigest()[:12], []
    for wire in sorted((Path(os.environ["HOME"]) / ".kimi-code/sessions").glob("wd_*_%s/session_*/agents/*/wire.jsonl" % key)):
        target = sdir / "wire" / wire.parents[2].name / (wire.parent.name + ".jsonl")
        target.parent.mkdir(parents=True, exist_ok=True); shutil.copyfile(wire, target); paths.append(target)
    return paths

def parse(paths):
    """Steps, tokens, tool calls and errors from wire.jsonl files (Kimi Code 2.0 nests loop events)."""
    m = dict(steps=0, request_kinds={}, models=[], efforts=[], tokens=dict.fromkeys(TOKENS, 0), tool_calls={}, calls=[], errors=[], agents_md=[])
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
                for name in TOKENS: m["tokens"][name] += int((record.get("usage") or {}).get(name) or 0)
            elif kind == "tool.call":
                name, args = str(record.get("name")), record.get("args")
                m["tool_calls"][name] = m["tool_calls"].get(name, 0) + 1
                m["calls"].append((name, args if isinstance(args, dict) else {"raw": args}))
            elif kind in ("turn.ended", "turn.step.retrying", "turn.step.interrupted") and (record.get("error") or record.get("errorName") or record.get("reason") == "error"):
                m["errors"].append({k: record.get(k) for k in ("type", "error", "errorName", "errorMessage", "statusCode", "message") if record.get(k) is not None})
            elif kind == "profile.bind": m["agents_md"] += record.get("agentsMdPaths") or []
    return m

def command_words(command):
    """The first word of every simple command, also inside sh -c and bash -c."""
    text = HEREDOC.sub(r"\1", command).replace("\n", ";")
    try: lexer = shlex.shlex(text, posix=True, punctuation_chars=True); lexer.whitespace_split = True; tokens = list(lexer)
    except ValueError: tokens = re.findall(r"[;|&()]+|[^\s;|&()]+", text)
    words, start = [], True
    for index, token in enumerate(tokens):
        if token in SEPARATORS: start = True
        elif start and not (re.fullmatch(r"\w+=.*", token) or token in PREFIXES):
            words.append(token)
            start = False
            if os.path.basename(token) in ("sh", "bash") and tokens[index + 1:index + 2] == ["-c"] and index + 2 < len(tokens): words += command_words(tokens[index + 2])
    return words

def path_tokens(command):
    """Words that name a file: home or temp variables, parent references, existing absolute roots."""
    found = []
    for token in re.findall(r"[^\s'\"`;|&<>()=,]+", HEREDOC.sub(r"\1", command)):
        head = token[1:].split("/")[0]
        if token.startswith(("~", "$HOME", "${HOME}", "$TMPDIR", "${TMPDIR}")) or token == ".." or token.startswith("../") or "/../" in token: found.append(token)
        elif token.startswith("/") and head and os.path.exists("/" + head): found.append(token)
    return found

def outside(token, work):
    if token.startswith(("~", "$HOME", "${HOME}", "$TMPDIR", "${TMPDIR}")): return True
    if token in ("/dev/null", "/dev/stdin", "/dev/stdout", "/dev/stderr") or token.startswith("/dev/fd/"): return False
    path = os.path.normpath(os.path.join(str(work), token))
    path = "/private" + path if path == "/tmp" or path.startswith("/tmp/") else path  # /tmp is a link to /private/tmp
    return path != str(work) and not path.startswith(str(work) + os.sep)

def audit(calls, work):
    """Deviations: a path outside the workspace, a compiler or evo outside check, or a web tool."""
    found = []
    for name, args in calls:
        if name in WEB or name.startswith("mcp__"): found.append(dict(kind="web", tool=name, detail=json.dumps(args)[:300]))
        elif name == "Bash":
            command = str(args.get("command", ""))
            for token in ([str(args["cwd"])] if args.get("cwd") else []) + path_tokens(command):
                if outside(token, work): found.append(dict(kind="path", tool=name, detail=token))
            for word in command_words(command):
                if COMPILER.fullmatch(os.path.basename(word)): found.append(dict(kind="compiler", tool=name, detail=command[:300]))
        elif name in FILE_TOOLS:
            for key in ("path", "file_path") + (("pattern",) if name == "Glob" else ()):  # a Grep pattern is a regex
                value = args.get(key)
                if isinstance(value, str) and (key != "pattern" or value.startswith(("/", "~", ".."))) and outside(value, work):
                    found.append(dict(kind="path", tool=name, detail=value))
    return found

def session(args, out, base, index, item, condition, prompt):
    sdir, work = workspace(args, out, index, item, condition)
    original, sealed = (item / "program.c").read_text(), digest(work / "check")
    row = dict(index=index, item=item.name, condition=condition, model=args.model, effort=args.effort, workspace=str(work), started_at=now())
    argv = [str(args.kimi), "-p", prompt, "--output-format", "stream-json", "-m", args.model, "--agent-file", str(base / "agent.md"), "--skills-dir", str(base / "skills")]
    status, stop, code, seconds = launch(argv, work, environment(args, base, sdir), sdir, args.wall)
    lines = (sdir / "stderr.txt").read_text(errors="replace").splitlines()  # tool output also goes to stderr
    fatal = [line for line in lines if line.startswith("error: failed to run prompt:")]
    final = (work / "program.c").read_text(errors="replace") if (work / "program.c").is_file() else ""
    logs = session_logs(work, sdir)
    m = parse(logs)
    stop = stop or next(filter(STOP.search, fatal), None) or next((json.dumps(e) for e in m["errors"] if refusal(e)), None)
    if stop: status = "stopped"
    elif status is None: status = "completed" if code == 0 else "step_limit" if any("max_steps_exceeded" in line for line in fatal) else "error"
    deviations = audit(m["calls"], work)
    if not (work / "check").is_file() or digest(work / "check") != sealed: deviations.append(dict(kind="check_modified", tool=None, detail="check"))
    checks = (sdir / "checks.jsonl").read_text().splitlines() if (sdir / "checks.jsonl").exists() else []
    bash = [str(a.get("command", "")) for n, a in m["calls"] if n == "Bash"]
    row.update(seconds=round(seconds, 1), exit_status=code, status=status, stop=stop, fatal=fatal[:5] or (lines[-3:] if code else []), session_dirs=sorted({p.parent.name for p in logs}),
               models_seen=m["models"], efforts_seen=m["efforts"], steps=m["steps"], request_kinds=m["request_kinds"],
               tokens=dict(m["tokens"], total=sum(m["tokens"].values())), tool_calls=m["tool_calls"],
               check_runs=sum(any(os.path.basename(w) == "check" for w in command_words(c)) for c in bash), check_invocations=len(checks),
               bash_commands=bash, deviations=deviations, errors=m["errors"][:20], agents_md=m["agents_md"],
               extra_files=sorted(set(os.listdir(work)) - {"program.c", "check"}), final_sha256=hashlib.sha256(final.encode()).hexdigest(),
               preprocessor=bool(re.search(r"#|%:|\?\?=", re.sub(r"\\\r?\n", "", final))))
    shutil.move(str(work), str(sdir / "workspace"))
    try:
        row["passed"], records = repair.score(final, item)
        row["score_exit_codes"] = [r["exit_code"] for r in records]
        metric, delta = repair.measure(original, final, json.loads((item / "fix-spans.json").read_text())["spans"])
        (sdir / "final.diff").write_text(delta)
        row.update(diff_size=metric["diff_added"] + metric["diff_removed"], diff_added=metric["diff_added"], diff_removed=metric["diff_removed"],
                   span_hits=metric["spans_touched"], all_spans=all(metric["spans_touched"]), changed_inside_spans=metric["changed_inside_spans"])
    except (repair.Failure, OSError, ValueError, KeyError) as error: row.update(passed=False, score_error=str(error))
    return row

def quota(kimi, port):
    """Plan quota from the local Kimi server (GET /api/v1/oauth/usage). The bearer token stays in memory."""
    env = dict(os.environ, KIMI_CODE_NO_AUTO_UPDATE="1", KIMI_DISABLE_TELEMETRY="1")
    server = subprocess.Popen([str(kimi), "web", "--no-open", "--port", str(port)], stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, env=env, start_new_session=True)
    base, result, token = "http://127.0.0.1:%d/api/v1/" % port, dict(time=now()), ""
    try:
        for _ in range(100):
            try: urllib.request.urlopen(base + "healthz", timeout=2).close(); break
            except OSError: time.sleep(0.3)
        token = (Path(os.environ["HOME"]) / ".kimi-code/server.token").read_text().strip()
        request = urllib.request.Request(base + "oauth/usage", headers={"Authorization": "Bearer " + token})
        with urllib.request.urlopen(request, timeout=30) as response: result["data"] = json.loads(response.read()).get("data")
    except (OSError, ValueError) as error: result["error"] = type(error).__name__
    finally: stop_process(server)
    if token and token in json.dumps(result): result = dict(time=result["time"], error="token_in_response")
    return result

def provenance(args):
    def text(*argv):
        try: return subprocess.run(argv, cwd=ROOT, capture_output=True, text=True, timeout=60).stdout.strip()
        except (OSError, subprocess.SubprocessError): return None
    try: image = json.loads(text(repair.CONTAINER, "image", "inspect", repair.IMAGE))[0]["configuration"]["descriptor"]["digest"]
    except (TypeError, ValueError, KeyError, IndexError): image = None
    return dict(commit=text("git", "rev-parse", "HEAD"), dirty=bool(text("git", "status", "--porcelain")), kimi=str(args.kimi),
                kimi_version=text(str(args.kimi), "--version"), kimi_sha256=digest(args.kimi), evo_sha256=digest(repair.EVO),
                image=repair.IMAGE, image_digest=image, harness_sha256=digest(HERE / "agent_repair.py"), repair_sha256=digest(HERE / "repair.py"),
                prompt_sha256=digest(args.prompt), python=sys.version.split()[0])

def totals(runs):
    total = dict(sessions=len(runs), passed=sum(r.get("passed") is True for r in runs), status={}, tool_calls={})
    for key in ("steps", "seconds", "check_runs", "check_invocations", "diff_size"): total[key] = round(sum(r.get(key) or 0 for r in runs), 1)
    total.update(tokens={k: sum(r.get("tokens", {}).get(k, 0) for r in runs) for k in TOKENS + ("total",)}, deviations=sum(len(r.get("deviations", [])) for r in runs),
                 sessions_with_deviations=sum(bool(r.get("deviations")) for r in runs))
    for r in runs:
        total["status"][r["status"]] = total["status"].get(r["status"], 0) + 1
        for name, count in r.get("tool_calls", {}).items(): total["tool_calls"][name] = total["tool_calls"].get(name, 0) + count
    return total

def write(out, receipt): (out / "receipt.json").write_text(json.dumps(receipt, indent=1) + "\n")

def archive(out):
    with tarfile.open(out / "wire.tar.gz", "w:gz") as tar:
        for path in sorted(out.glob("sessions/*/wire/*/*.jsonl")):
            info = tar.gettarinfo(str(path), arcname=str(path.relative_to(out)))
            info.mtime, info.uid, info.gid, info.uname, info.gname = 0, 0, 0, "", ""
            with open(path, "rb") as handle: tar.addfile(info, handle)

def run(args):
    conditions = args.condition or ["c0", "c1", "c3"]
    items = [args.items.resolve() / name for name in args.item]
    order = [(item, c) for rep in range(args.reps) for i, item in enumerate(items)
             for c in conditions[(i + rep) % len(conditions):] + conditions[:(i + rep) % len(conditions)]]
    if len(order) > args.max_sessions: raise SystemExit("the plan has %d sessions; --max-sessions is %d" % (len(order), args.max_sessions))
    if any(path.resolve().is_relative_to(ROOT) for path in (args.out, args.workspaces)): raise SystemExit("--out and --workspaces must be outside the repository")
    if any(not (item / "program.c").is_file() or not (item / "fix-spans.json").is_file() for item in items): raise SystemExit("an item is incomplete")
    plan = dict(order=[dict(item=i.name, condition=c) for i, c in order], items=str(args.items), model=args.model, effort=args.effort,
                wall_seconds=args.wall, max_steps=args.max_steps, max_sessions=args.max_sessions, reps=args.reps)
    if args.dry_run: print(json.dumps(plan)); return 0
    out, base, prompt = args.out.resolve(), args.out.resolve() / "env", args.prompt.read_text().strip()
    out.mkdir(parents=True); (base / "skills").mkdir(parents=True)
    args.workspaces = args.workspaces.resolve(); args.workspaces.mkdir(parents=True, exist_ok=True)
    (base / "agent.md").write_text(AGENT_FILE); executable(base / "login-shell", LOGIN_SHELL)
    controls = dict(argv=["<kimi>", "-p", "<prompt>", "--output-format", "stream-json", "-m", args.model, "--agent-file", "<agent.md>", "--skills-dir", "<empty dir>"],
                    env={k: v for k, v in environment(args, base, out).items() if k.startswith("KIMI_")}, path_tools=TOOLS + EXTRA_TOOLS + ["check"],
                    agent_file=AGENT_FILE, login_shell=LOGIN_SHELL)
    receipt = dict(schema="agent-repair.v3", phase=args.phase, date=now(), plan=plan, provenance=provenance(args), controls=controls,
                   prompt=prompt, quota=[], runs=[], totals=None, stop_reason=None)
    write(out, receipt)
    if args.quota_port: receipt["quota"].append(dict(label="before", **quota(args.kimi, args.quota_port)))
    for index, (item, condition) in enumerate(order, 1):
        try: row = session(args, out, base, index, item, condition, prompt)
        except Exception as error:  # record the infrastructure failure, then stop: no blind continuation
            row = dict(index=index, item=item.name, condition=condition, status="harness_error", error="%s: %s" % (type(error).__name__, error))
            receipt["stop_reason"] = "harness_error"
        receipt["runs"].append(row)
        if args.quota_port: receipt["quota"].append(dict(label="after %02d" % index, **quota(args.kimi, args.quota_port)))
        receipt["totals"] = totals(receipt["runs"])
        write(out, receipt)
        if row["status"] == "stopped": receipt["stop_reason"] = "provider_refusal"
        if receipt["stop_reason"]: break
    receipt.update(stop_reason=receipt["stop_reason"] or "completed", unattempted=len(order) - len(receipt["runs"]), totals=totals(receipt["runs"]))
    archive(out)
    write(out, receipt)
    print(json.dumps(dict(runs=len(receipt["runs"]), stop_reason=receipt["stop_reason"], totals=receipt["totals"])))
    return 0 if receipt["stop_reason"] == "completed" else 1

def main(argv=None):
    argv = sys.argv[1:] if argv is None else argv
    if argv[:1] == ["check"] and len(argv) >= 4: return check(argv[1], argv[2], argv[3], argv[4:])  # no argparse: agent arguments stay data
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("run",))
    parser.add_argument("--items", type=Path, required=True); parser.add_argument("--item", action="append", required=True)
    parser.add_argument("--condition", action="append", choices=("c0", "c1", "c3")); parser.add_argument("--reps", type=int, default=1)
    parser.add_argument("--max-sessions", type=int, default=9); parser.add_argument("--phase", default="pilot")
    parser.add_argument("--out", type=Path, required=True); parser.add_argument("--workspaces", type=Path, required=True)
    parser.add_argument("--kimi", type=Path, default=KIMI); parser.add_argument("--model", default="kimi-code/k3")
    parser.add_argument("--effort", default="low", help="KIMI_MODEL_THINKING_EFFORT, or 'default' to leave it unset")
    parser.add_argument("--wall", type=float, default=900); parser.add_argument("--max-steps", type=int, default=100)
    parser.add_argument("--prompt", type=Path, default=HERE / "prompt-3.txt"); parser.add_argument("--quota-port", type=int)
    parser.add_argument("--dry-run", action="store_true", help="print the plan; start no session")
    return run(parser.parse_args(argv))

if __name__ == "__main__":
    raise SystemExit(main())
