// Cobrust VSCode/Cursor extension — LSP client + DAP debugger wire-up.
//
// Responsibilities:
//   1. Register the `cobrust` language (declared in package.json).
//   2. Spawn the LSP server: prefer `cobrust lsp` subcommand (v0.6.0+ per
//      ADR-0068); fallback to `cobrust-lsp` standalone shim (v0.5.x).
//   3. Wire stdio LSP transport via `vscode-languageclient/node`.
//   4. Register the DAP debug adapter: prefer `cobrust dap` subcommand
//      (v0.6.0+); fallback to `cobrust-dap` standalone shim (v0.5.x).
//
// See ADR-0067 for original scaffold rationale and ADR-0068 for the
// subcommand collapse path.

import * as vscode from 'vscode';
import { workspace, window, ExtensionContext } from 'vscode';
import {
    LanguageClient,
    LanguageClientOptions,
    ServerOptions,
    TransportKind,
} from 'vscode-languageclient/node';

let client: LanguageClient | undefined;

class CobrustDebugAdapterFactory implements vscode.DebugAdapterDescriptorFactory {
    createDebugAdapterDescriptor(
        _session: vscode.DebugSession,
    ): vscode.ProviderResult<vscode.DebugAdapterDescriptor> {
        const cfg = workspace.getConfiguration('cobrust');
        const dapPath = cfg.get<string>('dapPath') || 'cobrust';
        const useSubcommand = cfg.get<boolean>('dap.useSubcommand', true);
        if (useSubcommand) {
            return new vscode.DebugAdapterExecutable(dapPath, ['dap']);
        }
        // Fallback: cobrust-dap standalone shim (v0.5.2 compat).
        return new vscode.DebugAdapterExecutable('cobrust-dap', []);
    }
}

export function activate(context: ExtensionContext): void {
    const cfg = workspace.getConfiguration('cobrust');
    const lspPath = cfg.get<string>('lspPath') || 'cobrust';
    const useLspSubcommand = cfg.get<boolean>('lsp.useSubcommand', true);
    const traceLevel = cfg.get<string>('trace.server') || 'off';

    const serverOptions: ServerOptions = useLspSubcommand
        ? {
              run: {
                  command: lspPath,
                  args: ['lsp'],
                  transport: TransportKind.stdio,
              },
              debug: {
                  command: lspPath,
                  args: ['lsp'],
                  transport: TransportKind.stdio,
              },
          }
        : {
              run: {
                  command: 'cobrust-lsp',
                  args: [],
                  transport: TransportKind.stdio,
              },
              debug: {
                  command: 'cobrust-lsp',
                  args: [],
                  transport: TransportKind.stdio,
              },
          };

    const clientOptions: LanguageClientOptions = {
        documentSelector: [{ scheme: 'file', language: 'cobrust' }],
        synchronize: {
            fileEvents: workspace.createFileSystemWatcher('**/*.cb'),
            configurationSection: 'cobrust',
        },
        outputChannelName: 'Cobrust LSP',
        traceOutputChannel: window.createOutputChannel('Cobrust LSP Trace'),
    };

    client = new LanguageClient(
        'cobrust',
        'Cobrust Language Server',
        serverOptions,
        clientOptions,
    );

    client.start().catch((err: unknown) => {
        const msg = err instanceof Error ? err.message : String(err);
        const hint = useLspSubcommand
            ? `Tried subcommand "${lspPath} lsp" (v0.6.0+ path). ` +
              `Ensure 'cobrust' is on $PATH, or disable 'cobrust.lsp.useSubcommand' ` +
              `to fall back to the 'cobrust-lsp' standalone shim (v0.5.x).`
            : `Tried standalone 'cobrust-lsp' (v0.5.x path). ` +
              `Ensure it is on $PATH, or set 'cobrust.lspPath' explicitly.`;
        window.showErrorMessage(`Cobrust LSP failed to start: ${msg}. ${hint}`);
    });

    context.subscriptions.push(
        vscode.debug.registerDebugAdapterDescriptorFactory(
            'cobrust',
            new CobrustDebugAdapterFactory(),
        ),
    );

    // Suppress unused-locals when noUnusedLocals is enabled; traceLevel is
    // surfaced for future trace plumbing without changing the public API.
    void traceLevel;
}

export function deactivate(): Thenable<void> | undefined {
    if (!client) {
        return undefined;
    }
    return client.stop();
}
