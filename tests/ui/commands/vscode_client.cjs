// Dependency-free adapter regression; this does not replace a real VS Code host.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const { EventEmitter } = require('node:events');

const root = path.resolve(__dirname, '../../..');
const source = fs.readFileSync(path.join(root, 'editors/vscode/extension.ts'), 'utf8');
const sent = [], events = {}, shown = new Map(), errors = [], failures = [];
const uri = value => ({ fsPath: new URL(value).pathname, toString: () => value });
const document = {
    uri: uri('file:///repo/example.c'), languageId: 'c', version: 1, isClosed: false,
    getText: () => '/* 😀 */ int f(int unused) { return 0; }\n'
};
const server = new EventEmitter();
server.stdout = new EventEmitter();
server.stderr = new EventEmitter();
server.stdin = new EventEmitter();
function emit(message) {
    const body = JSON.stringify(message);
    const frame = Buffer.from(`Content-Length: ${Buffer.byteLength(body)}\r\n\r\n${body}`);
    server.stdout.emit('data', frame.subarray(0, 7));
    server.stdout.emit('data', frame.subarray(7));
}
let actionResult;
server.stdin.write = frame => {
    const request = JSON.parse(frame.slice(frame.indexOf('\r\n\r\n') + 4));
    sent.push(request);
    if (request.id !== undefined) {
        const result = request.method === 'textDocument/codeAction' ? actionResult : null;
        setImmediate(() => emit({ jsonrpc: '2.0', id: request.id, result }));
    }
};
server.stdin.end = () => {};
server.kill = () => {};
class Range {
    constructor(sl, sc, el, ec) {
        this.start = { line: sl, character: sc };
        this.end = { line: el, character: ec };
    }
}
class Diagnostic {
    constructor(range, message, severity) { Object.assign(this, { range, message, severity }); }
}
class CodeAction {
    constructor(title, kind) { Object.assign(this, { title, kind }); }
}
class WorkspaceEdit {
    constructor() { this.edits = []; }
    replace(uri, range, text) { this.edits.push({ uri, range, text }); }
}
const disposable = () => ({ dispose() {} });
let provider;
const vscode = {
    Range, Diagnostic, CodeAction, WorkspaceEdit, Uri: { parse: uri },
    CodeActionKind: { QuickFix: 'quickfix' },
    window: {
        createOutputChannel: () => ({ ...disposable(), append() {}, appendLine() {} }),
        showErrorMessage: text => errors.push(text)
    },
    workspace: {
        textDocuments: [document], workspaceFolders: [{ uri: uri('file:///repo') }],
        getConfiguration: () => ({ get: () => '/repo/evo' })
    },
    languages: {
        match: (_, d) => d.languageId === 'c' ? 1 : 0,
        createDiagnosticCollection: () => ({
            ...disposable(), set: (uri, list) => shown.set(uri.toString(), list),
            delete: uri => shown.delete(uri.toString()), clear: () => shown.clear()
        }),
        registerCodeActionsProvider: (_, p) => { provider = p; return disposable(); }
    }
};
for (const name of ['Open', 'Change', 'Save', 'Close']) {
    vscode.workspace[`onDid${name}TextDocument`] = fn => { events[name] = fn; return disposable(); };
}
const extension = {};
vm.runInNewContext(source, {
    exports: extension, Buffer, process, setTimeout,
    require: name => name === 'vscode' ? vscode : {
        spawn: (executable, args) => {
            assert.equal(executable, '/repo/evo');
            assert.deepEqual(Array.from(args), ['lsp']);
            return server;
        }
    }
});
function check(name, body) {
    try { body(); console.log(`PASS: ${name}`); }
    catch (error) { failures.push(`${name}: ${error.message}`); }
}
async function run() {
    await extension.activate({ subscriptions: [] });
    check('initialization and already-open C document', () => {
        assert.deepEqual(sent.map(x => x.method), ['initialize', 'initialized', 'textDocument/didOpen']);
        assert.equal(sent[2].params.textDocument.text, document.getText());
    });
    const diagnostic = {
        range: { start: { line: 0, character: 19 }, end: { line: 0, character: 25 } },
        message: 'unused parameter', code: 'W0002', severity: 2, source: 'evocc'
    };
    const publish = (target, diagnostics, version) => emit({
        jsonrpc: '2.0', method: 'textDocument/publishDiagnostics',
        params: { uri: target, version, diagnostics }
    });
    publish(document.uri.toString(), [diagnostic], 1);
    check('split frames preserve UTF-16 diagnostics and warning severity', () => {
        assert.equal(shown.get(document.uri.toString())[0].range.start.character, 19);
        assert.equal(shown.get(document.uri.toString())[0].severity, 1);
    });
    publish('file:///repo/unopened.h', [diagnostic]);
    check('diagnostics for unopened headers remain visible', () => {
        assert.equal(shown.get('file:///repo/unopened.h')?.length, 1);
    });
    publish('file:///repo/unopened.h', []);
    check('unopened header diagnostics can be cleared', () => {
        assert.equal(shown.get('file:///repo/unopened.h')?.length, 0);
    });
    actionResult = [{
        title: 'Use unused', kind: 'quickfix', edit: { documentChanges: [{
            textDocument: { uri: document.uri.toString(), version: 1 },
            edits: [{ range: { start: { line: 0, character: 29 }, end: { line: 0, character: 29 } }, newText: '(void)unused; ' }]
        }] }
    }];
    const getActions = () => provider.provideCodeActions(document, new Range(0, 19, 0, 25),
        { diagnostics: shown.get(document.uri.toString()) }, { isCancellationRequested: false });
    const actions = await getActions();
    check('evocc diagnostic source reaches code-action request', () => {
        assert.equal(sent.at(-1).params.context.diagnostics[0]?.source, 'evocc');
    });
    check('safe versioned edit becomes an undoable editor edit', () => {
        assert.equal(actions.length, 1);
        assert.equal(actions[0].edit.edits[0].text, '(void)unused; ');
    });
    publish(document.uri.toString(), [{ ...diagnostic, source: 'evo' }], 1);
    await getActions();
    check('legacy evo diagnostic source remains supported', () => {
        assert.equal(sent.at(-1).params.context.diagnostics[0]?.source, 'evo');
    });
    actionResult[0].command = { command: 'unexpected' };
    const commandActions = await getActions();
    check('server commands are not exposed', () => assert.equal(commandActions.length, 0));
    delete actionResult[0].command;
    const stale = getActions(); document.version++;
    const staleActions = await stale;
    check('action response is refused after a local change', () => assert.equal(staleActions.length, 0));
    events.Change({ document, contentChanges: [{}] }); events.Save(document);
    check('changes and saves synchronize the full current buffer', () => {
        assert.equal(sent.at(-2).params.contentChanges[0].text, document.getText());
        assert.equal(sent.at(-2).params.textDocument.version, 2);
        assert.equal(sent.at(-1).params.text, document.getText());
    });
    publish(document.uri.toString(), [], 1);
    check('stale diagnostics for an open document are ignored', () => {
        assert.equal(shown.get(document.uri.toString()).length, 1);
    });
    events.Close(document);
    check('closing a C document removes its diagnostics', () => assert.equal(shown.has(document.uri.toString()), false));
    await extension.deactivate();
    check('shutdown precedes exit with no client errors', () => {
        assert.deepEqual(sent.slice(-2).map(x => x.method), ['shutdown', 'exit']);
        assert.equal(errors.length, 0);
    });
    assert.equal(failures.length, 0, failures.join('\n'));
}
run().catch(error => { console.error(error); process.exitCode = 1; });
