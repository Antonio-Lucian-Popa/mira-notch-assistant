# Security Policy

## Reporting a vulnerability

Please **do not** open a public issue for security problems.

Instead, use GitHub's **private vulnerability reporting** on this repository:
*Security → Report a vulnerability* (or the repo's *Security* tab → *Advisories*).
This opens a private channel with the maintainers.

Please include:

- a description of the issue and its impact,
- steps to reproduce (a proof of concept if possible),
- affected version / commit.

You can expect an acknowledgement within a few days. Once a fix is ready we'll
coordinate a disclosure date with you.

## Scope

Mira is local-first. It makes a single outbound network request — to
[LRCLIB](https://lrclib.net) for lyrics — and stores data only on your machine.
Reports about data exfiltration, privilege escalation, or the D-Bus surface
(`ro.novabytecode.Mira`) are especially welcome.

## Supported versions

The project is pre-1.0; only the latest `main` is supported. Please reproduce
issues against the current `main` before reporting.
