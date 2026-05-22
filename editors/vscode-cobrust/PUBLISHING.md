# Publishing the Cobrust VSCode/Cursor extension

> **This file documents user-side actions.** Per ADR-0067 §Decision, the
> CTO/agent does NOT execute the final `vsce publish` step; that requires a
> Personal Access Token (PAT) which must remain user-side and never enters
> the repo.

## Channels

The extension is intended for three channels:

| Channel | Audience | Tool | Required credential |
|---|---|---|---|
| GitHub release `.vsix` attachment | Offline installs, CI tests | `gh release upload` | GH token (CI) |
| VSCode Marketplace | VSCode users | `vsce publish` | Azure DevOps PAT, scope `Marketplace.publish` |
| Open VSX | Cursor + VSCodium users | `ovsx publish` | Open VSX token |

## One-time setup

### A. VSCode Marketplace publisher

1. Create or sign in to an Azure DevOps account: https://dev.azure.com
2. Visit https://marketplace.visualstudio.com/manage and create a
   publisher with ID `cobrust-lang` (must match `publisher` field in
   `package.json`).
3. In Azure DevOps user settings, create a Personal Access Token (PAT):
   - **Organization**: all accessible organizations
   - **Scopes** → **Custom defined** → check **Marketplace (publish)**
   - Expiry: 90 days (rotate)
4. Store the PAT in a password manager. **Never commit it.**

### B. Open VSX publisher

1. Sign in at https://open-vsx.org with GitHub.
2. Visit https://open-vsx.org/user-settings/namespaces and claim
   `cobrust-lang`.
3. Visit https://open-vsx.org/user-settings/tokens and generate a token.

## Per-release publishing flow

After landing a version bump (e.g., 0.1.0 → 0.1.1) in `package.json` and
`CHANGELOG.md`:

```bash
cd editors/vscode-cobrust

# 1. Build the .vsix
npm install
npm run compile
npx vsce package --no-dependencies
# yields cobrust-<VERSION>.vsix

# 2. Attach to the GitHub release (assumes a release tag exists)
gh release upload v<VERSION> cobrust-<VERSION>.vsix

# 3. Publish to VSCode Marketplace
npx vsce login cobrust-lang
# (paste your Azure DevOps PAT when prompted)
npx vsce publish <VERSION>

# 4. Publish to Open VSX (required for Cursor + VSCodium auto-update)
npx ovsx publish ./cobrust-<VERSION>.vsix -p <OPENVSX_TOKEN>
```

## Verifying the publish

- VSCode Marketplace listing:
  https://marketplace.visualstudio.com/items?itemName=cobrust-lang.cobrust
- Open VSX listing:
  https://open-vsx.org/extension/cobrust-lang/cobrust
- In editor: open the extensions pane, search "Cobrust", confirm publisher
  is `cobrust-lang` and version matches.

## Rollback / unpublish

- `npx vsce unpublish cobrust-lang.cobrust@<version>` (last 24h only)
- For older versions, contact Marketplace support.

## CI automation (future)

A GitHub Actions workflow scaffold for automated publish-on-tag is
out-of-scope for v0.1.0 (see ADR-0067 §Wave-1 OOS). When implemented, store
the PAT in `secrets.VSCE_PAT` and the Open VSX token in `secrets.OVSX_PAT`.
