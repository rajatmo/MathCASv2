// MathCAS language support for VS Code: a minimal Language Server Protocol client for mc-lsp (over its stdio, with
// Content-Length framing), and the commands "MathCAS: Run File" and "MathCAS: Run Selection" (mc-repl in a
// terminal). Plain JavaScript on the editor's own API and Node's standard modules: no dependencies.
'use strict';

const vscode = require('vscode');
const childProcess = require('child_process');
const fs = require('fs');
const path = require('path');

const LANGUAGE = 'mathcas';
const EXE = process.platform === 'win32' ? '.exe' : '';

/** @type {LspClient | null} */
let client = null;
/** @type {vscode.OutputChannel} */
let output;
/** @type {vscode.DiagnosticCollection} */
let diagnostics;

// ------------------------------------------------------------------------------------------------------------
// the protocol client

class LspClient {
  /**
   * @param {string} command the server executable
   * @param {string | undefined} cwd its working folder
   */
  constructor(command, cwd) {
    this.command = command;
    this.cwd = cwd;
    this.nextId = 0;
    /** @type {Map<number, {resolve: (v: any) => void, reject: (e: Error) => void}>} */
    this.pending = new Map();
    /** @type {Map<string, (params: any) => void>} */
    this.handlers = new Map();
    this.buffer = Buffer.alloc(0);
    this.running = false;
    this.proc = null;
  }

  /** Starts the server process; rejects if it cannot be started. */
  start() {
    return new Promise((resolve, reject) => {
      let proc;
      try {
        proc = childProcess.spawn(this.command, [], { cwd: this.cwd, stdio: ['pipe', 'pipe', 'pipe'] });
      } catch (e) {
        reject(e);
        return;
      }
      this.proc = proc;
      proc.on('error', (e) => {
        this.running = false;
        this.failAll(e);
        reject(e);
      });
      proc.on('spawn', () => {
        this.running = true;
        resolve();
      });
      proc.on('exit', (code) => {
        this.running = false;
        this.failAll(new Error(`mc-lsp exited with code ${code}`));
        if (this.onExit) this.onExit(code);
      });
      proc.stdout.on('data', (chunk) => this.receive(chunk));
      proc.stderr.on('data', (chunk) => output.append(chunk.toString('utf8')));
      proc.stdin.on('error', () => {});
    });
  }

  failAll(err) {
    for (const p of this.pending.values()) p.reject(err);
    this.pending.clear();
  }

  /** Splits the server's output into messages. */
  receive(chunk) {
    this.buffer = Buffer.concat([this.buffer, chunk]);
    for (;;) {
      const end = this.buffer.indexOf('\r\n\r\n');
      if (end < 0) return;
      const header = this.buffer.subarray(0, end).toString('ascii');
      const m = /Content-Length:\s*(\d+)/i.exec(header);
      if (!m) {
        // not a header: drop it and look for the next one
        this.buffer = this.buffer.subarray(end + 4);
        continue;
      }
      const length = Number(m[1]);
      if (this.buffer.length < end + 4 + length) return;
      const body = this.buffer.subarray(end + 4, end + 4 + length).toString('utf8');
      this.buffer = this.buffer.subarray(end + 4 + length);
      let msg;
      try {
        msg = JSON.parse(body);
      } catch (e) {
        output.appendLine(`mc-lsp sent a message that is not JSON: ${e}`);
        continue;
      }
      this.dispatch(msg);
    }
  }

  dispatch(msg) {
    if (msg.id !== undefined && msg.method === undefined) {
      const p = this.pending.get(msg.id);
      if (!p) return;
      this.pending.delete(msg.id);
      if (msg.error) p.reject(new Error(`${msg.error.message} (${msg.error.code})`));
      else p.resolve(msg.result);
      return;
    }
    if (msg.method === undefined) return;
    if (msg.id !== undefined) {
      // a request from the server: none is needed by this client
      this.send({ jsonrpc: '2.0', id: msg.id, error: { code: -32601, message: `unsupported: ${msg.method}` } });
      return;
    }
    const handler = this.handlers.get(msg.method);
    if (handler) handler(msg.params);
    else if (msg.method === 'window/logMessage' || msg.method === 'window/showMessage') output.appendLine(msg.params.message);
  }

  send(msg) {
    if (!this.running || !this.proc) return;
    const body = Buffer.from(JSON.stringify(msg), 'utf8');
    this.proc.stdin.write(`Content-Length: ${body.length}\r\n\r\n`);
    this.proc.stdin.write(body);
  }

  /**
   * A request; resolves with its result (`null` when cancelled).
   * @param {vscode.CancellationToken} [token]
   */
  request(method, params, token) {
    if (!this.running) return Promise.reject(new Error('mc-lsp is not running'));
    const id = ++this.nextId;
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      if (token) {
        token.onCancellationRequested(() => {
          if (this.pending.delete(id)) {
            this.notify('$/cancelRequest', { id });
            resolve(null);
          }
        });
      }
      this.send({ jsonrpc: '2.0', id, method, params });
    });
  }

  notify(method, params) {
    this.send({ jsonrpc: '2.0', method, params });
  }

  /** Shutdown and exit; the process is killed if it does not exit within two seconds. */
  async stop() {
    if (!this.running) return;
    this.onExit = undefined;
    const proc = this.proc;
    const kill = setTimeout(() => proc && proc.kill(), 2000);
    try {
      await this.request('shutdown', null);
      this.notify('exit', null);
    } catch (e) {
      if (proc) proc.kill();
    }
    await new Promise((resolve) => {
      if (!this.running) resolve();
      else proc.on('exit', () => resolve());
    });
    clearTimeout(kill);
  }
}

// ------------------------------------------------------------------------------------------------------------
// conversions between the protocol and the editor

const toPosition = (p) => new vscode.Position(p.line, p.character);
const toRange = (r) => new vscode.Range(toPosition(r.start), toPosition(r.end));
const fromPosition = (p) => ({ line: p.line, character: p.character });
const documentParams = (doc, pos) => ({ textDocument: { uri: doc.uri.toString() }, position: fromPosition(pos) });
const markdown = (c) => (typeof c === 'string' ? new vscode.MarkdownString(c) : new vscode.MarkdownString(c.value));

// the protocol's kinds start at 1, the editor's enums at 0
const toCompletionKind = (k) => (typeof k === 'number' ? k - 1 : vscode.CompletionItemKind.Text);
const toSymbolKind = (k) => (typeof k === 'number' ? k - 1 : vscode.SymbolKind.Variable);

function toDiagnostic(d) {
  const severity = typeof d.severity === 'number' ? d.severity - 1 : vscode.DiagnosticSeverity.Error;
  const diag = new vscode.Diagnostic(toRange(d.range), d.message, severity);
  diag.source = d.source;
  if (Array.isArray(d.relatedInformation)) {
    diag.relatedInformation = d.relatedInformation.map(
      (r) => new vscode.DiagnosticRelatedInformation(new vscode.Location(vscode.Uri.parse(r.location.uri), toRange(r.location.range)), r.message),
    );
  }
  return diag;
}

function toSymbol(s) {
  const sym = new vscode.DocumentSymbol(s.name, s.detail || '', toSymbolKind(s.kind), toRange(s.range), toRange(s.selectionRange));
  sym.children = (s.children || []).map(toSymbol);
  return sym;
}

function toCompletion(item) {
  const c = new vscode.CompletionItem(item.label, toCompletionKind(item.kind));
  if (item.detail) c.detail = item.detail;
  if (item.documentation) c.documentation = markdown(item.documentation);
  return c;
}

// ------------------------------------------------------------------------------------------------------------
// finding the programs

function settings() {
  return vscode.workspace.getConfiguration('mathcas');
}

/** The first `name` in a folder of PATH, or null. */
function onPath(name) {
  for (const dir of (process.env.PATH || '').split(path.delimiter)) {
    if (!dir) continue;
    const p = path.join(dir, name);
    try {
      if (fs.statSync(p).isFile()) return p;
    } catch (e) {
      // not there
    }
  }
  return null;
}

/** mc-repl: the setting, as a path or a name looked up on PATH. */
function replPath() {
  const configured = settings().get('repl.path') || 'mc-repl';
  if (path.isAbsolute(configured) || configured.includes('/') || configured.includes('\\')) return configured;
  return onPath(configured + (configured.endsWith(EXE) ? '' : EXE)) || configured;
}

/** mc-lsp: the setting, else next to mc-repl, else on PATH. */
function serverPath() {
  const configured = settings().get('server.path');
  if (configured) return configured;
  const repl = replPath();
  if (path.isAbsolute(repl)) {
    const sibling = path.join(path.dirname(repl), 'mc-lsp' + EXE);
    if (fs.existsSync(sibling)) return sibling;
  }
  return onPath('mc-lsp' + EXE) || 'mc-lsp';
}

// ------------------------------------------------------------------------------------------------------------
// the server's lifetime and document synchronisation

const isOurs = (doc) => doc.languageId === LANGUAGE;

function didOpen(doc) {
  if (client && isOurs(doc)) {
    client.notify('textDocument/didOpen', {
      textDocument: { uri: doc.uri.toString(), languageId: LANGUAGE, version: doc.version, text: doc.getText() },
    });
  }
}

async function startClient() {
  if (!settings().get('server.enable')) return;
  const command = serverPath();
  const folder = vscode.workspace.workspaceFolders && vscode.workspace.workspaceFolders[0];
  const c = new LspClient(command, folder ? folder.uri.fsPath : undefined);
  c.handlers.set('textDocument/publishDiagnostics', (p) => {
    diagnostics.set(vscode.Uri.parse(p.uri), (p.diagnostics || []).map(toDiagnostic));
  });
  try {
    await c.start();
    await c.request('initialize', {
      processId: process.pid,
      clientInfo: { name: 'vscode-mathcas' },
      rootUri: folder ? folder.uri.toString() : null,
      capabilities: {
        textDocument: {
          synchronization: { didSave: false },
          publishDiagnostics: { relatedInformation: true },
          hover: { contentFormat: ['markdown', 'plaintext'] },
          completion: { completionItem: { documentationFormat: ['markdown', 'plaintext'] } },
          documentSymbol: { hierarchicalDocumentSymbolSupport: true },
          definition: {},
        },
      },
    });
  } catch (e) {
    output.appendLine(`Cannot start the MathCAS language server "${command}": ${e.message}`);
    vscode.window.showWarningMessage(
      `MathCAS: cannot start the language server "${command}". Set "mathcas.server.path" or put mc-lsp on PATH.`,
    );
    return;
  }
  c.notify('initialized', {});
  c.onExit = (code) => {
    output.appendLine(`mc-lsp exited (code ${code}); use "MathCAS: Restart Language Server" to start it again.`);
    if (client === c) client = null;
  };
  client = c;
  output.appendLine(`Started ${command}`);
  vscode.workspace.textDocuments.forEach(didOpen);
}

async function stopClient() {
  const c = client;
  client = null;
  diagnostics.clear();
  if (c) await c.stop();
}

/** Asks the running server; resolves with null when there is none or it fails. */
async function ask(method, params, token) {
  if (!client) return null;
  try {
    return await client.request(method, params, token);
  } catch (e) {
    output.appendLine(`${method}: ${e.message}`);
    return null;
  }
}

// ------------------------------------------------------------------------------------------------------------
// commands

function terminal(name, args, cwd) {
  const t = vscode.window.createTerminal({ name, shellPath: replPath(), shellArgs: args, cwd });
  t.show();
  return t;
}

async function runFile() {
  const editor = vscode.window.activeTextEditor;
  if (!editor || !isOurs(editor.document)) {
    vscode.window.showInformationMessage('MathCAS: open a MathCAS file to run it.');
    return;
  }
  const doc = editor.document;
  if (doc.isUntitled) {
    vscode.window.showInformationMessage('MathCAS: save the file first (or use Run Selection).');
    return;
  }
  if (doc.isDirty && !(await doc.save())) return;
  const file = doc.uri.fsPath;
  terminal(`MathCAS: ${path.basename(file)}`, [file], path.dirname(file));
}

function runSelection() {
  const editor = vscode.window.activeTextEditor;
  if (!editor) return;
  const doc = editor.document;
  const sel = editor.selection;
  const text = (sel.isEmpty ? doc.lineAt(sel.active.line).text : doc.getText(sel)).trim();
  if (!text) {
    vscode.window.showInformationMessage('MathCAS: nothing to run (select source, or put the cursor on a line).');
    return;
  }
  // imports in the selection are looked up next to the file
  const cwd = doc.uri.scheme === 'file' ? path.dirname(doc.uri.fsPath) : undefined;
  terminal('MathCAS: selection', ['-e', text], cwd);
}

// ------------------------------------------------------------------------------------------------------------
// activation

async function activate(context) {
  output = vscode.window.createOutputChannel('MathCAS');
  diagnostics = vscode.languages.createDiagnosticCollection(LANGUAGE);
  const selector = [{ language: LANGUAGE }];
  context.subscriptions.push(
    output,
    diagnostics,
    vscode.commands.registerCommand('mathcas.runFile', runFile),
    vscode.commands.registerCommand('mathcas.runSelection', runSelection),
    vscode.commands.registerCommand('mathcas.restartServer', async () => {
      await stopClient();
      await startClient();
    }),
    vscode.workspace.onDidOpenTextDocument(didOpen),
    vscode.workspace.onDidChangeTextDocument((e) => {
      if (!client || !isOurs(e.document) || e.contentChanges.length === 0) return;
      client.notify('textDocument/didChange', {
        textDocument: { uri: e.document.uri.toString(), version: e.document.version },
        contentChanges: [{ text: e.document.getText() }],
      });
    }),
    vscode.workspace.onDidCloseTextDocument((doc) => {
      if (!isOurs(doc)) return;
      if (client) client.notify('textDocument/didClose', { textDocument: { uri: doc.uri.toString() } });
      diagnostics.delete(doc.uri);
    }),
    vscode.workspace.onDidChangeConfiguration(async (e) => {
      if (e.affectsConfiguration('mathcas.server') || e.affectsConfiguration('mathcas.repl')) {
        await stopClient();
        await startClient();
      }
    }),
    vscode.languages.registerDefinitionProvider(selector, {
      async provideDefinition(doc, pos, token) {
        const r = await ask('textDocument/definition', documentParams(doc, pos), token);
        if (!r) return null;
        const list = Array.isArray(r) ? r : [r];
        return list.map((l) => new vscode.Location(vscode.Uri.parse(l.uri), toRange(l.range)));
      },
    }),
    vscode.languages.registerHoverProvider(selector, {
      async provideHover(doc, pos, token) {
        const r = await ask('textDocument/hover', documentParams(doc, pos), token);
        if (!r || !r.contents) return null;
        return new vscode.Hover(markdown(r.contents), r.range ? toRange(r.range) : undefined);
      },
    }),
    vscode.languages.registerCompletionItemProvider(
      selector,
      {
        async provideCompletionItems(doc, pos, token) {
          const r = await ask('textDocument/completion', documentParams(doc, pos), token);
          if (!r) return null;
          const items = Array.isArray(r) ? r : r.items || [];
          return new vscode.CompletionList(items.map(toCompletion), !!r.isIncomplete);
        },
      },
      '.',
    ),
    vscode.languages.registerDocumentSymbolProvider(selector, {
      async provideDocumentSymbols(doc, token) {
        const r = await ask('textDocument/documentSymbol', { textDocument: { uri: doc.uri.toString() } }, token);
        return Array.isArray(r) ? r.map(toSymbol) : null;
      },
    }),
  );
  await startClient();
}

function deactivate() {
  return stopClient();
}

module.exports = { activate, deactivate };
