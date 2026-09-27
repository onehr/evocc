#!/usr/bin/env python3
"""Supplement musl's static lane with the five original loader/TLS subjects.

Reuse the configured tree and accepted objects; build only two additional loader
TUs. The shared result has its own composition and never erases static failures.
No source tree, installed archive, or reference object is overwritten.
"""
from __future__ import annotations

import json
from pathlib import Path
import subprocess

import mixed

SUBJECTS = ('functional/dlopen', 'functional/tls_align', 'functional/tls_align_dlopen',
            'functional/tls_init_dlopen', 'regression/tls_get_new-dtv')
PLUGINS = ('functional/dlopen_dso', 'functional/tls_align_dso',
           'functional/tls_init_dso', 'regression/tls_get_new-dtv_dso')
LOADERS = {'obj/ldso/dlstart.lo', 'obj/ldso/dynlink.lo'}

# This is the same configured Makefile recipe used by a shared musl build.
# -W forces only the two loader recipes and link to be printed even on replay.
PREPARE = r'''
import hashlib, json, pathlib, shlex, subprocess
out=pathlib.Path('/work/shared'); ref=out/'reference'; ref.mkdir()
p=subprocess.run(['make','-n','-W','ldso/dlstart.c','-W','ldso/dynlink.c','lib/libc.so'],
                 cwd='/work/musl',capture_output=True,text=True,check=True)
commands=[shlex.split(line) for line in p.stdout.replace('\\\n',' ').splitlines()
          if line.startswith('/work/musl/evo-recordcc ')]
compiles=[c for c in commands if '-c' in c and c[-1] in ('ldso/dlstart.c','ldso/dynlink.c')]
links=[c for c in commands if '-shared' in c and 'lib/libc.so' in c]
assert len(compiles)==2 and len(links)==1, 'missing/ambiguous shared build recipes'
commands=compiles+links
(out/'original-commands.json').write_text(json.dumps(commands,indent=2)+'\n')
rows=[]
for argv in commands:
    cmd=list(argv); cmd[0]='gcc-14'; name=pathlib.Path(cmd[cmd.index('-o')+1]).name
    cmd[cmd.index('-o')+1]=str(ref/name)
    if '-shared' in cmd:
        cmd=[str(ref/pathlib.Path(x).name) if x.startswith('obj/ldso/') else x for x in cmd]
    p=subprocess.run(cmd,cwd='/work/musl',capture_output=True,text=True,timeout=90)
    rows.append(dict(argv=cmd,exit=p.returncode,stdout=p.stdout,stderr=p.stderr))
    (out/'reference-build.json').write_text(json.dumps(rows,indent=2)+'\n')
    assert p.returncode==0,p.stderr
'''

LINK = r'''
import hashlib, json, pathlib, subprocess
out=pathlib.Path('/work/shared'); root=out/'candidate'; root.mkdir()
expected=json.loads((out/'expected-objects.json').read_text())
cmd=json.loads((out/'original-commands.json').read_text())[-1]
cmd[0]='gcc-14'; cmd[cmd.index('-o')+1]=str(root/'libc.so')
composition=[]
for i,part in enumerate(cmd):
    if not part.startswith('obj/'):
        continue
    if part.startswith('obj/ldso/'):
        path=out/'asm/obj'/(part.replace('/','__')+'.o'); compiler='evocc'
    elif part in expected:
        path=pathlib.Path('/work/candidate/obj')/(part.replace('/','__')+'.o'); compiler='evocc'
        assert hashlib.sha256(path.read_bytes()).hexdigest()==expected[part], part
    else:
        path=pathlib.Path('/work/musl')/part; compiler='reference'
    digest=hashlib.sha256(path.read_bytes()).hexdigest()
    composition.append(dict(unit=part,path=str(path),compiler=compiler,sha256=digest))
    cmd[i]=str(path)
p=subprocess.run(cmd,cwd='/work/musl',capture_output=True,text=True,timeout=90)
result=dict(argv=cmd,exit=p.returncode,stdout=p.stdout,stderr=p.stderr,composition=composition)
result['changedObjects']=[r['unit'] for r in composition
    if hashlib.sha256(pathlib.Path(r['path']).read_bytes()).hexdigest()!=r['sha256']]
(out/'candidate-build.json').write_text(json.dumps(result,indent=2)+'\n')
assert p.returncode==0 and not result['changedObjects'], p.stderr
'''

EXECUTE = r'''
import hashlib, json, pathlib, subprocess
out=pathlib.Path('/work/shared'); plan=json.loads((out/'subjects.json').read_text()); rows=[]
def digest(path): return hashlib.sha256(path.read_bytes()).hexdigest()
def run(cmd,cwd=None):
    try:
        p=subprocess.run(cmd,cwd=cwd,capture_output=True,text=True,timeout=15)
        return dict(argv=cmd,exit=p.returncode,stdout=p.stdout,stderr=p.stderr)
    except subprocess.TimeoutExpired as e:
        return dict(argv=cmd,exit=None,timeout=True,stdout=str(e.stdout),stderr=str(e.stderr))
identities={}
for library in ['reference','candidate']:
    root=out/library; so=root/'libc.so'; identities[library]=digest(so)
    for name in plan['plugins']:
        output=root/'src'/(name+'.so'); output.parent.mkdir(parents=True,exist_ok=True)
        obj=output.with_suffix('.lo')
        compile=['gcc-14','-nostdinc','-isystem','/work/musl/sysroot/include',
                 '-I/work/tests/src/common','-std=c99','-D_POSIX_C_SOURCE=200809L',
                 '-fno-builtin','-frounding-math','-g','-fPIC','-DSHARED',
                 '-c','/work/tests/src/'+name+'.c','-o',str(obj)]
        compiled=run(compile)
        cmd=['gcc-14','-nostdlib','-shared',str(obj),'/work/tests/src/common/libtest.a',
             str(so),'-lgcc','-lgcc_eh','-o',str(output)]
        rows.append(dict(library=library,plugin=name,compile=compiled,build=run(cmd)))
    for name in plan['subjects']:
        output=root/'src'/(name+'.exe'); output.parent.mkdir(parents=True,exist_ok=True)
        obj=pathlib.Path('/work/tests/src')/(name+'.o')
        cmd=['gcc-14','-nostdlib','-no-pie','/work/musl/obj/crt/crt1.o',
             '/work/musl/obj/crt/aarch64/crti.o',str(obj),'/work/tests/src/common/libtest.a',
             str(so),'-lgcc','-lgcc_eh','/work/musl/obj/crt/aarch64/crtn.o',
             '-Wl,--dynamic-linker,'+str(so),'-Wl,-rpath,$ORIGIN','-rdynamic','-o',str(output)]
        if name=='functional/tls_align': cmd.append(str(root/'src/functional/tls_align_dso.so'))
        row=dict(library=library,subject=name,objectSha256=digest(obj),build=run(cmd))
        if row['build']['exit']==0:
            row['elf']=run(['readelf','-lWd',str(output)])
            row['binarySha256']=digest(output)
            row['run']=run(['/work/tests/reference-runtest',str(output)],str(root))
        rows.append(row)
    assert digest(so)==identities[library], 'shared library replaced during tests'
    (out/'executions.json').write_text(json.dumps(dict(rows=rows,libraryHashes=identities),indent=2)+'\n')
composition=json.loads((out/'candidate-build.json').read_text())['composition']
assert all(digest(pathlib.Path(r['path']))==r['sha256'] for r in composition), 'object replaced'
'''


def validate_executions(data: dict) -> bool:
    """No aggregate success marker can replace the exact paired population."""
    expected = {(side, name) for side in ('reference', 'candidate') for name in SUBJECTS}
    expected_plugins = {(side, name) for side in ('reference', 'candidate') for name in PLUGINS}
    seen, plugins = set(), set()
    if set(data.get('libraryHashes', {})) != {'reference', 'candidate'}:
        return False
    for row in data.get('rows', []):
        side = row.get('library')
        if row.get('build', {}).get('exit') != 0:
            return False
        if 'plugin' in row:
            key = side, row['plugin']
            if key in plugins or row.get('compile', {}).get('exit') != 0:
                return False
            plugins.add(key)
        elif 'subject' in row:
            key = side, row['subject']
            if key in seen or row.get('run', {}).get('exit') != 0:
                return False
            elf = row.get('elf', {})
            if (elf.get('exit') != 0 or f'[Requesting program interpreter: /work/shared/{side}/libc.so]' not in elf.get('stdout', '')
                    or not row.get('binarySha256') or not row.get('objectSha256')):
                return False
            seen.add(key)
        else:
            return False
    return seen == expected and plugins == expected_plugins


def run(evocc: Path, tree: Path, tests: Path, candidates: Path,
        expected: dict[str, str], output: Path) -> dict:
    output.mkdir(parents=True, exist_ok=False)
    result = {'scope': 'five original shared-loader subjects; not full musl',
              'status': 'ERROR', 'compilerSha256': mixed.sha256(evocc),
              'evidence': str(output), 'subjects': list(SUBJECTS),
              'referenceTestHarness': 'GCC-built original subjects/plugins and preserved five-second supervisor'}
    mounts = {'shared': output, 'musl': tree, 'tests': tests, 'candidate': candidates}

    def stage(name, script):
        done = mixed.container(mounts, "python3 - <<'PY'\n" + script + "\nPY", timeout=240)
        (output / (name + '-runner.json')).write_text(json.dumps({
            'exit': done.returncode, 'stdout': done.stdout, 'stderr': done.stderr}, indent=2) + '\n')
        if done.returncode != 0:
            raise ValueError(f'{name} runner exited {done.returncode}')

    try:
        stage('prepare', PREPARE)
        commands = json.loads((output / 'original-commands.json').read_text())
        units = [mixed.Unit('/work/musl', tuple(c[1:]), c[-1], c[c.index('-o') + 1], '/work/musl')
                 for c in commands[:-1]]
        if {u.name for u in units} != LOADERS or len(units) != len(LOADERS):
            raise ValueError('incorrect loader compilation population')
        rows = mixed.emit_all(units, evocc, 'musl', tree, output / 'asm', jobs=1, timeout=90)
        (output / 'loader-units.json').write_text(json.dumps(rows, indent=2) + '\n')
        verdicts = mixed.assemble(rows, {u.name: u for u in units}, output / 'asm')
        result['loaderUnits'] = rows
        if verdicts != dict.fromkeys(LOADERS, 'ok'):
            result['status'] = 'REFUSED'
            return result
        (output / 'expected-objects.json').write_text(json.dumps(expected, indent=2) + '\n')
        stage('link', LINK)
        composition = json.loads((output / 'candidate-build.json').read_text())['composition']
        used = {r['unit'] for r in composition if r['compiler'] == 'evocc'}
        permitted = {u for u in expected if u.startswith('obj/src/')} | LOADERS
        if used != permitted:
            raise ValueError('candidate DSO lost or added an accepted object')
        result['objectCounts'] = {c: sum(r['compiler'] == c for r in composition)
                                  for c in ('evocc', 'reference')}
        result['compositionSha256'] = mixed.sha256(output / 'candidate-build.json')
        (output / 'subjects.json').write_text(json.dumps({'subjects': SUBJECTS, 'plugins': PLUGINS}) + '\n')
        stage('execute', EXECUTE)
        data = json.loads((output / 'executions.json').read_text())
        result.update(status='PASS' if validate_executions(data) else 'FAIL',
                      libraryHashes=data['libraryHashes'],
                      executionsSha256=mixed.sha256(output / 'executions.json'))
        return result
    except (OSError, ValueError, KeyError, subprocess.TimeoutExpired) as error:
        result['error'] = str(error)
        return result
    finally:
        (output / 'summary.json').write_text(json.dumps(result, indent=2) + '\n')
