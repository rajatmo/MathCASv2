// Drives editors/vscode/extension.js against the real mc-lsp with a stand-in for the editor's API, as far as the
// extension uses it: activation, document synchronisation, diagnostics, definition, hover, completion, symbols,
// the run commands and shutdown. Run by tests/extension.rs when a JavaScript runtime is available:
//   node smoke.js <extension.js> <mc-lsp> <scratch folder>
'use strict';
const Module = require('module');
const fs = require('fs');
const path = require('path');

const [extensionFile, serverFile, scratch] = process.argv.slice(2);
const fail = (msg) => {
  console.error(`FAIL: ${msg}`);
  process.exit(1);
};
setTimeout(() => fail('timed out'), 30000).unref();

// -------------------------------------------------------------------------------------------------------------
// the stand-in API

const listeners = { open: [], change: [], close: [], config: [] };
const providers = {};
const commands = {};
const terminals = [];
const published = new Map();
const waiting = [];
const disposable = { dispose() {} };

function fileUri(p) {
  const slashed = p.replace(/\\/g, '/');
  const s = 'file://' + (slashed.startsWith('/') ? '' : '/') + slashed.split('/').map(encodeURIComponent).join('/');
  return { scheme: 'file', fsPath: p, toString: () => s };
}

class Position {
  constructor(line, character) {
    this.line = line;
    this.character = character;
  }
}
class Range {
  constructor(start, end) {
    this.start = start;
    this.end = end;
  }
}

const vscode = {
  Position,
  Range,
  Location: class {
    constructor(uri, range) {
      this.uri = uri;
      this.range = range;
    }
  },
  Uri: {
    file: fileUri,
    parse: (s) => ({ scheme: s.split(':')[0], fsPath: decodeURIComponent(s.replace(/^file:\/\/\/?/, '')), toString: () => s }),
  },
  MarkdownString: class {
    constructor(value) {
      this.value = value;
    }
  },
  Hover: class {
    constructor(contents, range) {
      this.contents = contents;
      this.range = range;
    }
  },
  CompletionItem: class {
    constructor(label, kind) {
      this.label = label;
      this.kind = kind;
    }
  },
  CompletionList: class {
    constructor(items, isIncomplete) {
      this.items = items;
      this.isIncomplete = isIncomplete;
    }
  },
  CompletionItemKind: { Text: 0 },
  SignatureHelp: class {
    constructor() {
      Object.assign(this, { signatures: [], activeSignature: 0, activeParameter: 0 });
    }
  },
  SignatureInformation: class {
    constructor(label, documentation) {
      Object.assign(this, { label, documentation, parameters: [] });
    }
  },
  ParameterInformation: class {
    constructor(label) {
      this.label = label;
    }
  },
  SymbolKind: { Variable: 12 },
  DocumentSymbol: class {
    constructor(name, detail, kind, range, selectionRange) {
      Object.assign(this, { name, detail, kind, range, selectionRange, children: [] });
    }
  },
  Diagnostic: class {
    constructor(range, message, severity) {
      Object.assign(this, { range, message, severity });
    }
  },
  DiagnosticSeverity: { Error: 0 },
  DiagnosticRelatedInformation: class {
    constructor(location, message) {
      Object.assign(this, { location, message });
    }
  },
  window: {
    activeTextEditor: undefined,
    createOutputChannel: () => ({ append() {}, appendLine() {}, dispose() {} }),
    showWarningMessage: (m) => fail(`warning shown: ${m}`),
    showInformationMessage: () => {},
    createTerminal: (options) => {
      terminals.push(options);
      return { show() {} };
    },
  },
  workspace: {
    workspaceFolders: undefined,
    textDocuments: [],
    getConfiguration: () => ({
      get: (key) => ({ 'server.enable': true, 'server.path': serverFile, 'repl.path': 'mc-repl' })[key],
    }),
    onDidOpenTextDocument: (f) => (listeners.open.push(f), disposable),
    onDidChangeTextDocument: (f) => (listeners.change.push(f), disposable),
    onDidCloseTextDocument: (f) => (listeners.close.push(f), disposable),
    onDidChangeConfiguration: (f) => (listeners.config.push(f), disposable),
  },
  languages: {
    createDiagnosticCollection: () => ({
      set(uri, list) {
        published.set(uri.toString(), list);
        waiting.splice(0).forEach((w) => w());
      },
      delete() {},
      clear() {},
      dispose() {},
    }),
    registerDefinitionProvider: (_, p) => ((providers.definition = p), disposable),
    registerHoverProvider: (_, p) => ((providers.hover = p), disposable),
    registerCompletionItemProvider: (_, p, ...triggers) => ((providers.completion = p), (providers.triggers = triggers), disposable),
    registerDocumentSymbolProvider: (_, p) => ((providers.symbols = p), disposable),
    registerSignatureHelpProvider: (_, p, meta) => ((providers.signature = p), (providers.signatureMeta = meta), disposable),
  },
  commands: { registerCommand: (name, f) => ((commands[name] = f), disposable) },
};

const load = Module._load;
Module._load = function (request, ...rest) {
  return request === 'vscode' ? vscode : load.call(this, request, ...rest);
};

function documentOf(file, text, version) {
  const lines = text.split('\n');
  return {
    uri: fileUri(file),
    fileName: file,
    languageId: 'mathcas',
    version,
    isUntitled: false,
    isDirty: false,
    getText: (range) => (range ? lines[range.start.line].slice(range.start.character, range.end.character) : text),
    lineAt: (line) => ({ text: lines[line] }),
  };
}

/** The next diagnostics published (call right after the notification that causes them). */
function nextDiagnostics(uri) {
  return new Promise((resolve) => {
    const check = () => (published.has(uri) ? resolve(published.get(uri)) : waiting.push(check));
    published.delete(uri);
    waiting.push(check);
  });
}

const never = { onCancellationRequested() {} };

// -------------------------------------------------------------------------------------------------------------
// the run

(async () => {
  const ext = require(extensionFile);
  const context = { subscriptions: [] };
  await ext.activate(context);
  if (providers.triggers[0] !== '.') fail('completion is not triggered by "."');
  if (!providers.signature || providers.signatureMeta.triggerCharacters[0] !== '(') fail('no signature help provider');
  for (const c of ['mathcas.runFile', 'mathcas.runSelection', 'mathcas.restartServer', 'mathcas.openDocs']) {
    if (!commands[c]) fail(`command ${c} not registered`);
  }

  fs.writeFileSync(path.join(scratch, 'Geo.mc'), 'module Geo {\n  # the area of a circle\n  pub fn area(r) = pi*r^2\n}\n');
  const main = path.join(scratch, 'main.mc');
  const bad = 'import Geo\ny = 2 "x"\n';
  let doc = documentOf(main, bad, 1);
  const uri = doc.uri.toString();
  listeners.open.forEach((f) => f(doc));
  const errs = await nextDiagnostics(uri);
  if (errs.length !== 1 || errs[0].range.start.line !== 1 || errs[0].severity !== 0) fail(`diagnostics ${JSON.stringify(errs)}`);

  const good = 'import Geo\ny = area(2) + Geo.\n';
  doc = documentOf(main, good, 2);
  listeners.change.forEach((f) => f({ document: doc, contentChanges: [{}] }));
  if ((await nextDiagnostics(uri)).length !== 1) fail('no diagnostic for the incomplete `Geo.`');

  const def = await providers.definition.provideDefinition(doc, new Position(1, 5), never);
  if (!def || def.length !== 1 || !def[0].uri.toString().endsWith('/Geo.mc') || def[0].range.start.line !== 2) {
    fail(`definition ${JSON.stringify(def)}`);
  }
  const hover = await providers.hover.provideHover(doc, new Position(1, 5), never);
  if (!hover || !hover.contents.value.includes('pub fn area(r)') || !hover.contents.value.includes('area of a circle')) {
    fail(`hover ${JSON.stringify(hover)}`);
  }
  const list = await providers.completion.provideCompletionItems(doc, new Position(1, 18), never);
  if (!list || list.items.map((i) => i.label).join() !== 'area' || list.items[0].kind !== 2) {
    fail(`completion ${JSON.stringify(list)}`);
  }
  // signature help inside a call of a documented function, the second parameter active
  doc = documentOf(main, 'y = n_integrate(sin(x), [x, 0, 1])\n', 3);
  listeners.change.forEach((f) => f({ document: doc, contentChanges: [{}] }));
  await nextDiagnostics(uri);
  const sig = await providers.signature.provideSignatureHelp(doc, new Position(0, 24), never);
  if (!sig || !sig.signatures[sig.activeSignature].label.startsWith('n_integrate(') || sig.activeParameter !== 1) {
    fail(`signature help ${JSON.stringify(sig)}`);
  }
  // a documented function's hover links to its page through the trusted command
  const fhover = await providers.hover.provideHover(doc, new Position(0, 6), never);
  if (!fhover || !fhover.contents.value.includes('command:mathcas.openDocs') || !fhover.contents.isTrusted) {
    fail(`documented hover ${JSON.stringify(fhover)}`);
  }
  doc = documentOf(main, 'fn f(x) = x\nmodule M {\n  pub fn p(x) = x\n}\n', 4);
  listeners.change.forEach((f) => f({ document: doc, contentChanges: [{}] }));
  if ((await nextDiagnostics(uri)).length !== 0) fail('diagnostics for a correct text');
  const symbols = await providers.symbols.provideDocumentSymbols(doc, never);
  if (!symbols || symbols.map((s) => s.name).join() !== 'f,M' || symbols[1].children[0].name !== 'p' || symbols[1].kind !== 1) {
    fail(`symbols ${JSON.stringify(symbols)}`);
  }

  // the run commands start mc-repl in a terminal
  const selection = { isEmpty: false, active: new Position(0, 0), start: new Position(0, 0), end: new Position(0, 11) };
  vscode.window.activeTextEditor = { document: doc, selection };
  commands['mathcas.runSelection']();
  await commands['mathcas.runFile']();
  if (terminals.length !== 2) fail(`terminals ${JSON.stringify(terminals)}`);
  if (JSON.stringify(terminals[0].shellArgs) !== JSON.stringify(['-e', 'fn f(x) = x'])) fail(`run selection ${JSON.stringify(terminals[0])}`);
  if (JSON.stringify(terminals[1].shellArgs) !== JSON.stringify([main]) || path.resolve(terminals[1].cwd) !== path.resolve(scratch)) {
    fail(`run file ${JSON.stringify(terminals[1])}`);
  }

  listeners.close.forEach((f) => f(doc));
  await ext.deactivate();
  console.log('ok');
  process.exit(0);
})().catch((e) => fail(e && e.stack ? e.stack : String(e)));
