# Security Policy

## Reporting a Vulnerability

If you discover a security vulnerability in chattor, **please do not open a
public issue.** Instead, report it privately:

1. **Email:** Send a detailed report to the maintainers via the email listed in
   the repository's GitHub profile
2. **GitHub:** Use GitHub's
   [private vulnerability reporting](https://docs.github.com/en/code-security/security-advisories/guidance-on-reporting-and-writing-information-about-vulnerabilities/privately-reporting-a-security-vulnerability)
   feature on this repository

Include as much detail as possible:
- Description of the vulnerability
- Steps to reproduce
- Potential impact
- Suggested fix (if you have one)

## Response Timeline

- **Acknowledgment:** Within 72 hours of receiving your report
- **Assessment:** We'll confirm the issue and determine severity within 1 week
- **Fix:** Security patches are prioritized over feature work
- **Disclosure:** We'll coordinate with you on public disclosure timing

## Scope

The following areas are in scope for security reports:

- **Signal Protocol implementation** — key exchange, Double Ratchet, message
  encryption/decryption
- **Database encryption** — SQLCipher key derivation, at-rest encryption
- **Identity and key management** — Ed25519 keypair generation, storage, .onion
  derivation
- **Tor integration** — hidden service configuration, connection handling
- **Protocol handling** — message parsing, deserialization, input validation
- **Local data leakage** — unencrypted data in logs, temp files, or memory

## Out of Scope

- Vulnerabilities in upstream dependencies (report those to the respective
  projects, but do let us know so we can track and update)
- Tor network-level attacks (those are Tor Project's domain)
- Physical access attacks (if someone has your machine, all bets are off)
- Social engineering

## Supported Versions

| Version | Supported |
|---------|-----------|
| 0.1.x   | Yes       |

Only the latest release receives security updates.

## Credit

We're happy to credit security researchers in release notes and changelogs
(unless you prefer to remain anonymous). Let us know your preference when
reporting.
