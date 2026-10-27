# Windows Authenticode signing with SignPath Foundation

The integration is prepared, not activated. Current local `.exe` builds remain
unsigned until SignPath has approved the project and the repository has been
configured. No certificate or token is committed. Do not use self-signed
certificates or disable SmartScreen as a public distribution solution.

## Owner setup

1. Apply to [SignPath Foundation](https://signpath.org/) for this public MIT
   project. Review their [eligibility conditions](https://signpath.org/terms).
   Account creation, legal terms, identity/project approval and any signing
   approvals must be completed by the project owner. No application has been
   submitted automatically.
2. Connect the GitHub repository as a trusted build source in SignPath. Restrict
   production policies to the protected `main` branch and reviewed release tags;
   require the appropriate human signing approver(s). See
   [SignPath's GitHub integration](https://docs.signpath.io/trusted-build-systems/github).
3. Create two artifact configurations in the approved project:
   - slug `mira-components`: import [components.xml](signpath/components.xml).
   - slug `mira-installer`: import [installer.xml](signpath/installer.xml).
   Both inputs are the ZIP archives produced by `actions/upload-artifact`.
4. Configure repository **variables**:
   - `SIGNPATH_ORGANIZATION_ID`
   - `SIGNPATH_PROJECT_SLUG`
   - `SIGNPATH_POLICY_SLUG` (approved production signing policy)
   - `SIGNPATH_CERTIFICATE_SUBJECT` (the complete expected signing certificate
     subject, from the approved certificate; Foundation certificates identify
     SignPath Foundation as publisher, not automatically the app author's name).
5. Configure repository **secret** `SIGNPATH_API_TOKEN` with the scoped SignPath
   submitter token. Never paste this token into source files, logs or chat.
6. After a test-signed workflow has been reviewed, set repository variable
   `SIGNPATH_ENABLED=true`. Once enabled, missing configuration, failed signing,
   wrong publisher, invalid signatures or missing timestamps fail the Windows
   job and prevent publishing the release. There is no silent unsigned fallback.

Before activation, the workflow still produces development builds and publishes
`windows-signing-status.json` with `signed: false`. This is intentionally explicit.
A build reported as signed must have `signed: true` and pass independent Windows
signature verification.

## Signing order

1. Build `mira-win.exe` from the reviewed source.
2. Run Inno Setup with `SignedUninstaller=yes` and an empty uninstaller cache.
   The documented external-signing flow creates the unsigned `uninst-*.e32` or
   `.e64` template, then stops at the signature-required check. This preparation
   failure is expected; its output is never a release installer. Any other
   failure, extra/missing templates or timeout stops the job.
3. Upload the app plus the uninstaller template as `mira-win.exe` and
   `mira-uninstaller.exe`; SignPath signs them using `mira-components`.
4. Verify the certificate chain, exact expected publisher and timestamp. Restore
   the signed uninstaller at its original Inno cache path and the signed app at
   `target/release/mira-win.exe`.
5. Compile the installer again with `SignedUninstaller=yes`. Inno validates and
   embeds the signed uninstaller and its signed temporary self-copy code. It
   also embeds the already-signed Mira executable.
6. Upload the final outer setup as `setup.exe`; SignPath signs it using
   `mira-installer`. Verify again, then collect the signed app and installer.

The official SignPath action is pinned to the inspected v3.0 commit
`f6d04783b4569d051e0c80105fe66e82819d0092`. Input artifacts and the source job are
verified by SignPath's trusted-build connector. The Windows job has only
`contents: read` and `actions: read`; publishing is isolated to the release job.

The two-pass uninstaller flow follows
[Inno's SignedUninstaller documentation](https://jrsoftware.org/ishelp/topic_setup_signeduninstaller.htm).
It must be smoke-tested on the actual `windows-latest` runner and approved by
SignPath before production activation. This Linux environment cannot perform
real Authenticode signing or run the installer/uninstaller.

## Owner verification on Windows

Inspect the published signing status, then run:

```powershell
Get-AuthenticodeSignature .\mira-win.exe | Format-List Status,SignerCertificate,TimeStamperCertificate
Get-AuthenticodeSignature .\mira_0.5.0_windows_x64_setup.exe | Format-List Status,SignerCertificate,TimeStamperCertificate
# After installing, inspect the generated uninstaller too:
Get-AuthenticodeSignature "$env:LOCALAPPDATA\Programs\Mira\unins000.exe"
```

Test installation, upgrade, launch and uninstall as a standard user. Verify
that the embedded app, uninstaller and setup publisher match the approved
certificate and that Windows trusts their chains. Use the Windows SDK SignTool
(`signtool verify /pa /all /tw /v <file>`) for an additional independent check.

## SmartScreen is separate from a valid signature

Authenticode identifies the publisher and detects tampering. A new signed app
can still receive SmartScreen warnings while reputation builds. OV and EV
certificates do not guarantee instant SmartScreen reputation. SignPath approval
also does not guarantee that every enterprise policy permits the installer.
See [Microsoft's signing guidance](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/code-signing-options).
