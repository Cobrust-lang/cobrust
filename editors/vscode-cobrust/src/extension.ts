// Cobrust VSCode/Cursor extension — LSP client wire-up.
//
// Responsibilities:
//   1. Register the `cobrust` language (declared in package.json).
//   2. Spawn the `cobrust-lsp` binary (configurable via `cobrust.lspPath`).
//   3. Wire stdio LSP transport via `vscode-languageclient/node`.
//
// See ADR-0067 for design rationale and scope boundaries.

import { workspace, window, ExtensionContext } from 'vscode';
import {
    LanguageClient,
    LanguageClientOptions,
    ServerOptions,
    TransportKind,
} from 'vscode-languageclient/node';

let client: LanguageClient | undefined;

export function activate(context: ExtensionContext): void {
    const cfg = workspace.getConfiguration('cobrust');
    const lspPath = cfg.get<string>('lspPath') || 'cobrust-lsp';
    const traceLevel = cfg.get<string>('trace.server') || 'off';

    const serverOptions: ServerOptions = {
        run: {
            command: lspPath,
            transport: TransportKind.stdio,
        },
        debug: {
            command: lspPath,
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
        const msg =
            err instanceof Error ? err.message : String(err);
        window.showErrorMessage(
            `Cobrust LSP failed to start (lspPath="${lspPath}"): ${msg}. ` +
                `Ensure 'cobrust-lsp' is installed and on $PATH, or set ` +
                `'cobrust.lspPath' in settings.`,
        );
    });

    // Suppress unused-locals when noUnusedLocals is enabled; traceLevel is
    // surfaced for future trace plumbing without changing the public API.
    void traceLevel;
    void context;
}

export function deactivate(): Thenable<void> | undefined {
    if (!client) {
        return undefined;
    }
    return client.stop();
}
