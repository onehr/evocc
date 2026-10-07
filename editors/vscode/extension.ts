const vscode = require('vscode');
const { spawn } = require('node:child_process');
let stop = async () => {};
const wireRange = r => ({ start: { line: r.start.line, character: r.start.character }, end: { line: r.end.line, character: r.end.character } });
const editorRange = r => new vscode.Range(r.start.line, r.start.character, r.end.line, r.end.character);
exports.activate = async context => {
    const output = vscode.window.createOutputChannel('EvoCC'), diagnostics = vscode.languages.createDiagnosticCollection('evo');
    const server = spawn(vscode.workspace.getConfiguration('evo').get('path', 'evo'), ['lsp'], { cwd: vscode.workspace.workspaceFolders?.[0]?.uri.fsPath });
    const pending = new Map(), selector = { language: 'c', scheme: 'file' };
    let buffer = Buffer.alloc(0), serial = 0, dead = false, stopping = false;
    const fail = error => {
        if (dead) return; dead = true; output.appendLine(String(error)); diagnostics.clear();
        for (const call of pending.values()) call.reject(error); pending.clear(); server.kill();
        if (!stopping) vscode.window.showErrorMessage('EvoCC language server stopped. Check evo.path and the EvoCC output.');
    };
    const send = (method, params, id = undefined) => {
        if (dead) throw new Error('EvoCC language server is not running');
        const body = JSON.stringify({ jsonrpc: '2.0', method, params, ...(id === undefined ? {} : { id }) });
        server.stdin.write(`Content-Length: ${Buffer.byteLength(body)}\r\n\r\n${body}`);
    };
    const request = (method, params) => new Promise((resolve, reject) => { const id = ++serial; pending.set(id, { resolve, reject }); try { send(method, params, id); } catch (e) { pending.delete(id); reject(e); } });
    server.on('error', fail); server.stdin.on('error', fail); server.stderr.on('data', data => output.append(data.toString()));
    server.on('close', code => { if (!stopping) fail(new Error(`evo lsp exited with ${code}`)); });
    server.stdout.on('data', data => {
        try {
            buffer = Buffer.concat([buffer, data]);
            for (;;) {
                const end = buffer.indexOf('\r\n\r\n');
                if (end < 0) { if (buffer.length > 8192) throw new Error('Invalid LSP header'); return; }
                const lengths = [...buffer.subarray(0, end).toString('ascii').matchAll(/^Content-Length: (\d+)\r?$/gmi)];
                const length = lengths.length === 1 ? Number(lengths[0][1]) : -1;
                if (length < 0 || length > 1048576) throw new Error('Invalid LSP frame length');
                if (buffer.length < end + 4 + length) return;
                const message = JSON.parse(buffer.subarray(end + 4, end + 4 + length).toString('utf8')); buffer = buffer.subarray(end + 4 + length);
                if (pending.has(message.id)) { const call = pending.get(message.id); pending.delete(message.id); message.error ? call.reject(new Error(message.error.message)) : call.resolve(message.result); }
                if (message.method !== 'textDocument/publishDiagnostics') continue;
                const p = message.params, document = vscode.workspace.textDocuments.find(d => d.uri.toString() === p.uri);
                if (document && p.version !== undefined && p.version !== document.version) continue;
                diagnostics.set(vscode.Uri.parse(p.uri), p.diagnostics.map(d => { const item = new vscode.Diagnostic(editorRange(d.range), d.message, (d.severity || 1) - 1); item.code = d.code; item.source = d.source || 'evo'; return item; }));
            }
        } catch (error) { fail(error); }
    });
    stop = async () => { stopping = true; try { await Promise.race([request('shutdown', null), new Promise(resolve => setTimeout(resolve, 1000))]); send('exit', null); } catch {} server.stdin.end(); server.kill(); };
    context.subscriptions.push(output, diagnostics, { dispose: () => { stopping = true; server.kill(); } });
    try { await request('initialize', { processId: process.pid, rootUri: vscode.workspace.workspaceFolders?.[0]?.uri.toString() || null, capabilities: { general: { positionEncodings: ['utf-16'] }, textDocument: { synchronization: { didSave: true }, codeAction: { codeActionLiteralSupport: { codeActionKind: { valueSet: ['quickfix'] } } } } }, clientInfo: { name: 'evo-vscode', version: '0.1.0' } }); } catch (error) { fail(error); return; }
    send('initialized', {});
    const matches = d => vscode.languages.match(selector, d) > 0, identity = d => ({ uri: d.uri.toString() });
    const notify = (method, params) => { if (!dead) send(method, params); };
    const open = d => { if (matches(d)) notify('textDocument/didOpen', { textDocument: { ...identity(d), languageId: 'c', version: d.version, text: d.getText() } }); };
    context.subscriptions.push(vscode.workspace.onDidOpenTextDocument(open));
    context.subscriptions.push(vscode.workspace.onDidChangeTextDocument(e => { if (matches(e.document) && e.contentChanges.length) notify('textDocument/didChange', { textDocument: { ...identity(e.document), version: e.document.version }, contentChanges: [{ text: e.document.getText() }] }); }));
    context.subscriptions.push(vscode.workspace.onDidSaveTextDocument(d => { if (matches(d)) notify('textDocument/didSave', { textDocument: identity(d), text: d.getText() }); }));
    context.subscriptions.push(vscode.workspace.onDidCloseTextDocument(d => { if (matches(d)) { notify('textDocument/didClose', { textDocument: identity(d) }); diagnostics.delete(d.uri); } }));
    context.subscriptions.push(vscode.languages.registerCodeActionsProvider(selector, {
        async provideCodeActions(document, range, actionContext, token) {
            const version = document.version, uri = document.uri.toString();
            try {
                const context = { diagnostics: actionContext.diagnostics.filter(d => ['evo', 'evocc'].includes(d.source)).map(d => ({ range: wireRange(d.range), message: d.message, code: d.code, severity: d.severity + 1, source: d.source })) };
                const actions = await request('textDocument/codeAction', { textDocument: { uri }, range: wireRange(range), context });
                if (token.isCancellationRequested || document.version !== version || document.isClosed) return [];
                return (actions || []).flatMap(a => {
                    const changes = a.edit?.documentChanges;
                    if (a.kind !== 'quickfix' || a.command || !changes?.length || changes.some(c => c.textDocument?.uri !== uri || c.textDocument.version !== version || !Array.isArray(c.edits))) return [];
                    const action = new vscode.CodeAction(a.title, vscode.CodeActionKind.QuickFix); action.edit = new vscode.WorkspaceEdit();
                    for (const change of changes) for (const edit of change.edits) action.edit.replace(document.uri, editorRange(edit.range), edit.newText);
                    return [action];
                });
            } catch (error) { output.appendLine(String(error)); return []; }
        }
    }, { providedCodeActionKinds: [vscode.CodeActionKind.QuickFix] }));
    vscode.workspace.textDocuments.forEach(open);
};
exports.deactivate = () => stop();
