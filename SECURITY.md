# Security policy

Kea is early-alpha software. Security fixes target the latest development version;
older alpha releases do not have a separate maintenance commitment.

## Report a vulnerability

Use [GitHub private vulnerability reporting](https://github.com/ManuelZierl/kea/security/advisories/new)
when available. If it is unavailable, open an issue asking @ManuelZierl for a
private security-reporting channel, without including vulnerability details.
Do not open a public issue containing exploit details or secrets.

Include the affected version, platform, reproduction steps, expected security
boundary and likely impact. Use synthetic credentials and sample recordings.

## Data boundaries

Kea runs commands with the user's permissions; it is not a sandbox. Saved sessions,
persisted drafts and memories are unencrypted and can contain sensitive text.
Shell metadata is interoperability data, not authenticated provenance. Historical
replay must never execute commands or emit terminal side effects.
