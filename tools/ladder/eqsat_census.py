"""R02 diagnostic census of R01a records; not a performance admission gate.

This consumes trusted producer telemetry. Stored VERIFIED labels do not replace
independent archive replay, and absent/rejected chains never mean zero cap hits.
Counts are transaction-local, not unique-source optimization coverage.
"""
from __future__ import annotations

import fcntl
import json
import os
from pathlib import Path
import selectors
import shutil
import signal
import subprocess
import time

from .eqsat_chain import digest, keys, natural, proof_bucket, require, sha256_identity, unique, validate_record

DEFAULT_SCHEDULE = dict(max_nodes=100000, max_epochs=1024, max_transactions=8)
SCOPES = ('search.bridge_nodes', 'search.rewrite_events',
          'admission.accepted_regions', 'admission.proof_nodes')
BUCKETS = ('proof_successes', 'completed_proof_rejections',
           'bundle_obligation_failures', 'unknown_proof_outcomes', 'not_checked')


def chain_census(record):
    """No partial/error chain can enter the all-default no-hit denominator."""
    validate_record(record)
    require(record['policy']['schedule'] == DEFAULT_SCHEDULE, 'not the default coupled caps')
    require(record['policy']['registry'] == 'p0.scalar.v4'
            and record['policy']['cost_weights'] == [1] * 13, 'not the fixed default policy')
    observed = record['publication'] == 'VERIFIED'
    txs = record['transactions']
    buckets = {bucket: 0 for bucket in BUCKETS}
    for tx in txs:
        bucket = proof_bucket(tx['verification'], tx['rejection_reason'], tx['result'])
        # An additive, separately named recovery count preserves historical
        # no-recovery receipts without rewriting their frozen accounting.
        buckets[bucket] = buckets.get(bucket, 0) + 1
    rows = []
    for tx in txs:
        counters = tx['scoped_counters']
        search, admission = counters['search'], counters['admission']
        rows.append(dict(
            index=tx['index'], result=tx['result'], stop=tx['stop'],
            search_bridge_nodes=search['bridge_nodes'],
            search_rewrite_events=search['rewrite_events'],
            admitted_regions=admission['regions'], admitted_proof_nodes=admission['proof_nodes'],
            roots=tx['roots'], budget_hits=tx['budget_hits'], evp=counters['evp'],
            matching_work=counters['matching_work']))
    return dict(
        status='OBSERVED' if observed else 'CHAIN_REJECTED',
        **({} if observed else {'chain_rejection_reason': record['rejection_reason']}),
        input_digest=record['input_digest'], output_digest=record['output_digest'],
        chain_id=record['chain_id'], policy_digest=record['policy_digest'],
        attempted_transactions=len(txs), verified_prefix=record['verified_prefix'],
        hit_transactions={scope: sum(any(h['scope'] == scope for h in t['budget_hits'])
                                     for t in txs) if observed else None for scope in SCOPES},
        chain_limit_hit=(record['stop'] == 'TRANSACTION_LIMIT') if observed else None,
        matching_work=None, proof_accounting=buckets, transactions=rows)


def report_census(report):
    """Reject stale schemas, omitted programs and disagreement with chain IDs."""
    keys(report, 'schema policy corpusSource corpusDigest seed stepBudget programs aggregate')
    require(report['schema'] == 'evocc-policy-report-v2', 'missing scoped telemetry schema')
    require(type(report['corpusSource']) is str and report['corpusSource'], 'missing corpus identity')
    sha256_identity(report['corpusDigest'])
    natural(report['seed'])
    require(natural(report['stepBudget']) > 0, 'missing interpreter fuel')
    keys(report['policy'], 'name registry costFunction schedule')
    require(report['policy']['name'] == 'instruction-count'
            and report['policy']['registry'] == 'p0.scalar.v4'
            and report['policy']['costFunction'] == 'InstructionCount', 'not default policy')
    require(report['policy']['schedule'] == dict(maxNodes=100000, maxEpochs=1024,
                                               maxTransactions=8), 'not default schedule')
    programs = report['programs']
    require(type(programs) is list and programs, 'empty census population')
    names = set()
    rows = []
    outcomes = dict(measured=0, undefinedInput=0, fuelExhausted=0, refused=0)
    totals = dict(programs=len(programs), programsOptimized=0, stepsBefore=0, stepsAfter=0,
                  instructionsBefore=0, instructionsAfter=0, regions=0, transactions=0,
                  certificates=0, certificatesReplayed=0)
    wall = 0
    wall_elided = None
    for p in programs:
        keys(p, 'name inputDigest outputDigest outcome outcomeDetail stepsBefore stepsAfter '
                'instructionsBefore instructionsAfter regions transactions certificates '
                'certificatesReplayed optimizerWallNanos chainRecord')
        require(type(p['name']) is str and p['name'] and p['name'] not in names, 'duplicate program identity')
        names.add(p['name'])
        for field in ('inputDigest', 'outputDigest'):
            sha256_identity(p[field])
        require(p['outcome'] in outcomes, 'unknown interpreter outcome')
        require(type(p['outcomeDetail']) is str, 'missing outcome detail')
        outcomes[p['outcome']] += 1
        for field in ('stepsBefore', 'stepsAfter', 'instructionsBefore', 'instructionsAfter',
                      'regions', 'transactions', 'certificates', 'certificatesReplayed'):
            totals[field] += natural(p[field])
        totals['programsOptimized'] += int(p['regions'] != 0)
        elided = p['optimizerWallNanos'] is None
        require(wall_elided is None or wall_elided == elided, 'mixed timing collection modes')
        wall_elided = elided
        if not elided:
            wall += natural(p['optimizerWallNanos'])
        chain = p['chainRecord']
        if chain is None:
            require(p['outcome'] != 'measured', 'measured program has no verified chain')
            require(p['transactions'] == p['certificates'] == p['certificatesReplayed'] == 0,
                    'missing chain hides attempted transactions')
            rows.append(dict(name=p['name'], status='NO_CHAIN', outcome=p['outcome'],
                             input_digest=p['inputDigest'], hit_transactions=None,
                             chain_limit_hit=None, matching_work=None))
            continue
        row = chain_census(chain)
        require(chain['input_digest'] == p['inputDigest'], 'wrong chain input')
        require(p['transactions'] == p['certificates'] == len(chain['transactions'])
                and p['certificatesReplayed'] == sum(
                    t['verification'] == 'VERIFIED' for t in chain['transactions']),
                'incomplete replay accounting')
        if chain['publication'] == 'VERIFIED':
            if p['outcome'] == 'measured':
                require(chain['output_digest'] == p['outputDigest'], 'wrong chain output')
            require(p['regions'] == sum(t['roots']['accepted'] for t in chain['transactions']),
                    'wrong retained-region total')
        else:
            require(p['outcome'] == 'refused', 'rejected chain reported as measured')
        row.update(name=p['name'], outcome=p['outcome'])
        if p['outcome'] != 'measured':
            row.update(status='EXECUTION_NOT_ADMITTED', hit_transactions=None, chain_limit_hit=None)
        rows.append(row)
    expected = {**totals, **outcomes, 'optimizerWallNanos': None if wall_elided else wall}
    keys(report['aggregate'], ' '.join(expected))
    for key, value in report['aggregate'].items():
        if key != 'optimizerWallNanos' or value is not None:
            natural(value)
    require(report['aggregate'] == expected, 'aggregate omits or invents program outcomes')
    admitted = [row for row in rows if row['status'] == 'OBSERVED']
    hits = {scope: sum(row['hit_transactions'][scope] > 0 for row in admitted) for scope in SCOPES}
    return dict(
        schema='evocc-cap-census-v1', scope='producer-telemetry-not-performance-admission',
        corpus_digest=report['corpusDigest'], corpus_source=report['corpusSource'],
        attempted_programs=len(rows), observed_programs=len(admitted),
        unavailable_programs=len(rows) - len(admitted),
        attempted_transactions=sum(row.get('attempted_transactions', 0) for row in rows),
        programs_with_hits=hits,
        programs_at_chain_limit=sum(row['chain_limit_hit'] for row in admitted),
        default_caps_observation=('INCOMPLETE' if len(admitted) != len(rows) else
                                  'HITS_OBSERVED' if any(hits.values()) or any(
                                      row['chain_limit_hit'] for row in admitted) else
                                  'NO_HITS_IN_THIS_POPULATION'),
        matching_work=None, policy_preflight='NOT_RUN', rule_preflight='NOT_RUN',
        shortcut_preflight='NOT_RUN', rows=rows)


def validate_smoke(manifest):
    keys(manifest, 'schema scope implementation_commit evaluator checker generator generated seed '
                   'step_budget policy resources claim_boundary')
    require(manifest['schema'] == 'evocc-cap-smoke-manifest-v1'
            and manifest['scope'] == 'generated-ir-regression-only', 'unsupported measurement scope')
    require(manifest['policy'] == 'instruction-count', 'smoke requires the default policy')
    require(manifest['claim_boundary'] == 'not-R02-performance-or-track-admission', 'claim boundary changed')
    commit = manifest['implementation_commit']
    require(type(commit) is str and len(commit) == 40 and all(c in '0123456789abcdef' for c in commit),
            'missing implementation commit')
    for field in ('evaluator', 'checker', 'generator'):
        keys(manifest[field], 'path sha256')
        sha256_identity(manifest[field]['sha256'])
        path = Path(manifest[field]['path'])
        require(not path.is_absolute() and '..' not in path.parts, 'unscoped subject path')
    require(Path(manifest['evaluator']['path']).name == 'evo-policy-eval'
            and Path(manifest['checker']['path']).name == 'evo-proofcheck'
            and Path(manifest['evaluator']['path']).parent == Path(manifest['checker']['path']).parent,
            'checker must be the sibling selected by the shared evaluator')
    require(manifest['generator']['path'] == 'crates/evocc-randgate/src/generate.rs', 'unknown generator')
    keys(manifest['generated'], 'first count')
    first, count = (natural(manifest['generated'][key]) for key in ('first', 'count'))
    require(1 <= count <= 64 and first + count <= 1 << 64, 'unbounded or wrapped corpus')
    natural(manifest['seed'])
    require(1 <= natural(manifest['step_budget']) <= 200000, 'invalid interpreter bound')
    keys(manifest['resources'], 'timeout_seconds output_bytes min_free_bytes')
    r = manifest['resources']
    require(1 <= natural(r['timeout_seconds']) <= 300, 'invalid watchdog')
    require(1024 <= natural(r['output_bytes']) <= 16 * 1024 * 1024, 'invalid retention bound')
    require(natural(r['min_free_bytes']) >= 1024 ** 3, 'insufficient free-space reserve')


def run_smoke(manifest_path, root):
    """One frozen diagnostic using the existing evaluator and benchmark lock.

    No builders, container creation, native timing, model call or cleanup of
    caller-owned artifacts. Output is retained on every started attempt.
    """
    manifest_path, root = Path(manifest_path).resolve(), Path(root).resolve()
    require(manifest_path.is_relative_to(root / 'tests/contracts'), 'manifest must be tracked under tests/contracts')
    raw = manifest_path.read_bytes()
    manifest = json.loads(raw, object_pairs_hook=unique)
    validate_smoke(manifest)
    relative = manifest_path.relative_to(root).as_posix()
    committed = subprocess.run(['git', 'show', 'HEAD:' + relative], cwd=root, capture_output=True, check=False)
    require(committed.returncode == 0 and committed.stdout == raw, 'commit the frozen manifest before execution')
    fields = ('evaluator', 'checker', 'generator')

    def stable():
        require(manifest_path.read_bytes() == raw, 'manifest changed during collection')
        # A frozen binary hash is an artifact identity, not a source identity.
        # Check the claimed implementation revision against the live build inputs
        # as well; collector/manifest-only follow-up commits remain permitted.
        source_paths = ['Cargo.toml', 'Cargo.lock', 'crates', 'vendor', '.cargo', 'rust-toolchain.toml']
        revision = subprocess.run(['git', 'rev-parse', manifest['implementation_commit'] + '^{commit}'],
                                  cwd=root, capture_output=True, check=False)
        require(revision.returncode == 0 and revision.stdout.decode().strip() == manifest['implementation_commit'],
                'implementation commit is not a repository commit')
        diff = subprocess.run(['git', 'diff', '--quiet', manifest['implementation_commit'], '--', *source_paths],
                              cwd=root, capture_output=True, check=False)
        untracked = subprocess.run(['git', 'ls-files', '--others', '--exclude-standard', '--', *source_paths],
                                   cwd=root, capture_output=True, check=False)
        require(diff.returncode == 0 and untracked.returncode == 0 and not untracked.stdout,
                'implementation commit does not match live build inputs')
        for field in fields:
            item = manifest[field]
            require(digest((root / item['path']).read_bytes()) == item['sha256'], field + ' identity changed')

    stable()
    build = root / '.build'
    build.mkdir(exist_ok=True)
    with (build / 'bench.lock').open('a') as lock:
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError as error:
            raise ValueError('another benchmark owns the shared lock') from error
        stable()
        r = manifest['resources']
        require(shutil.disk_usage(root).free >= r['min_free_bytes'], 'disk reserve not available')
        base = root / '.evidence/eqsat-cap-smoke'
        base.mkdir(parents=True, exist_ok=True)
        directory = base / (time.strftime('%Y%m%dT%H%M%SZ', time.gmtime()) + '-' + str(time.time_ns()))
        directory.mkdir()  # exclusive; never adopt, overwrite or clean an old run
        (directory / 'manifest.json').write_bytes(raw)
        generated = manifest['generated']
        argv = [str(root / manifest['evaluator']['path']), '--policy', 'instruction-count',
                '--generated', f"{generated['first']}:{generated['count']}",
                '--seed', str(manifest['seed']), '--steps', str(manifest['step_budget'])]
        receipt = dict(schema='evocc-cap-smoke-receipt-v1', manifest_digest=digest(raw),
                       implementation_commit=manifest['implementation_commit'], argv=argv,
                       scope=manifest['scope'], status='STARTED', census=None,
                       native='NOT_RUN', source_lowering='NOT_APPLICABLE',
                       collector_files={name: digest((Path(__file__).parent / name).read_bytes())
                                        for name in ('eqsat_census.py', 'eqsat_chain.py')},
                       pipeline='generated-EvoIR/shared-chain/standalone-checker/EvoIR-interpreter')
        receipt_path = directory / 'receipt.json'

        def save():
            receipt_path.write_text(json.dumps(receipt, sort_keys=True, indent=2) + '\n')

        save()
        started = time.monotonic()
        try:
            with (directory / 'policy.json').open('wb') as out, (directory / 'stderr.txt').open('wb') as err:
                receipt['exit_code'] = bounded_capture(argv, root, out, err, r, started,
                    {**os.environ, 'EVOCC_CHECKER_SHA256': manifest['checker']['sha256']})
            require((directory / 'policy.json').stat().st_size + (directory / 'stderr.txt').stat().st_size
                    <= r['output_bytes'], 'output limit exceeded')
            require(receipt['exit_code'] == 0, 'evaluator did not complete')
            stable()
            report_bytes = (directory / 'policy.json').read_bytes()
            report = json.loads(report_bytes, object_pairs_hook=unique)
            require([p['name'] for p in report['programs']] == [
                f'seed-{seed:020}' for seed in range(generated['first'], generated['first'] + generated['count'])],
                'frozen corpus changed or missing programs')
            require(report['seed'] == manifest['seed'] and report['stepBudget'] == manifest['step_budget'],
                    'requested execution profile was not consumed')
            for p in report['programs']:
                c = p['chainRecord']
                if c is not None:
                    require(c['checker_digest'] == manifest['checker']['sha256'], 'unbound checker')
            census = report_census(report)
            receipt.update(status='OBSERVED' if census['unavailable_programs'] == 0 else 'INCOMPLETE',
                           census=census, policy_report_digest=digest(report_bytes))
        except (OSError, ValueError, KeyError, TypeError) as error:
            receipt.update(status='COLLECTION_FAILED', error=str(error))
        except BaseException:
            receipt.update(status='INTERRUPTED')
            raise
        finally:
            receipt['collector_wall_nanos'] = int((time.monotonic() - started) * 1_000_000_000)
            save()
        return directory, receipt


def bounded_capture(argv, root, out, err, resources, started, env):
    """Bound captured streams only; descendants may write larger proof files.

    Pipes provide backpressure. Never retain more than the shared byte cap;
    on overflow/deadline/disk failure kill the session even if its leader has
    exited while a descendant still holds a pipe open.
    """
    child = subprocess.Popen(argv, cwd=root, stdin=subprocess.DEVNULL,
                             stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                             start_new_session=True, env=env)
    retained = 0
    completed = False
    try:
        with selectors.DefaultSelector() as streams:
            for source, destination in ((child.stdout, out), (child.stderr, err)):
                os.set_blocking(source.fileno(), False)
                streams.register(source, selectors.EVENT_READ, destination)
            while streams.get_map() or child.poll() is None:
                require(time.monotonic() - started < resources['timeout_seconds'], 'watchdog expired')
                require(shutil.disk_usage(root).free >= resources['min_free_bytes'], 'disk reserve exhausted')
                for key, _ in streams.select(timeout=0.02):
                    data = os.read(key.fd, 65536)
                    if not data:
                        streams.unregister(key.fileobj)
                        continue
                    remaining = resources['output_bytes'] - retained
                    key.data.write(data[:remaining])
                    retained += min(len(data), remaining)
                    require(len(data) <= remaining, 'output limit exceeded')
        completed = True
        return child.wait()
    finally:
        if not completed:
            try:
                os.killpg(child.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
        child.wait()
        child.stdout.close()
        child.stderr.close()
