# Chattor Website Design

**Date:** 2026-02-19
**Status:** Approved

## Overview

A project website for chattor with a landing page and full documentation site, built with Astro Starlight and hosted on Netlify. Rose Pine dark theme to match the project's visual identity and logo.

## Decisions

| Decision | Choice |
|---|---|
| **Framework** | Astro with Starlight (docs framework) |
| **Location** | `website/` directory in the chattor monorepo |
| **Visual theme** | Rose Pine (dark variant), mapped to Starlight CSS custom properties |
| **Landing page** | Custom `index.astro` bypassing Starlight docs layout |
| **Docs pages** | 9 Markdown pages across 4 sidebar groups |
| **Hosting** | Netlify (subdomain initially, custom domain later) |
| **Content source** | Adapted from README, CLAUDE.md, and `docs/plans/` |
| **Search** | Pagefind (built into Starlight) |
| **Domain strategy** | `site` field in `astro.config.mjs` — update once when custom domain is ready |

## Project Structure

```
website/
├── astro.config.mjs          # Astro + Starlight config
├── package.json
├── netlify.toml               # Build config for Netlify
├── tsconfig.json
├── public/
│   └── chattor-logo.png      # Logo (copied from assets/)
├── src/
│   ├── assets/                # Images referenced in Astro components
│   ├── components/
│   │   └── Landing.astro      # Custom landing page (hero + features)
│   ├── content/
│   │   └── docs/
│   │       ├── getting-started/
│   │       │   ├── installation.md
│   │       │   └── quickstart.md
│   │       ├── guides/
│   │       │   ├── theming.md
│   │       │   └── friend-codes.md
│   │       ├── architecture/
│   │       │   ├── overview.md
│   │       │   ├── signal-protocol.md
│   │       │   └── tor-integration.md
│   │       ├── faq.md
│   │       └── contributing.md
│   ├── content.config.ts      # Starlight content collections
│   └── styles/
│       └── custom.css         # Rose Pine overrides
```

## Landing Page

The landing page (`index.astro`) uses a custom Astro component, not Starlight's docs layout.

### Hero Section
- Chattor logo (centered, prominent)
- Tagline: "Peer-to-peer encrypted chat over Tor, right in your terminal."
- Two CTA buttons: **Get Started** (→ installation docs) and **View on GitHub** (→ repo)
- Dark Rose Pine base background (`#191724`) with subtle decorative elements

### Features Grid
Six features in a 3-column grid (stacked on mobile):

1. **Signal Protocol E2E** — X3DH + ChaCha20-Poly1305, no plaintext fallback
2. **Pure P2P over Tor** — each user hosts their own onion service
3. **Encrypted at rest** — SQLCipher database, no unencrypted data on disk
4. **Broadcast channels** — signed posts, pull-based sync, read receipts
5. **7 terminal themes** — including Rose Pine variants, TOML customizable
6. **Offline delivery** — queued messages with exponential backoff retry

### Architecture Diagram
Simplified visual: `You → TUI → Signal Protocol → Tor → Peer`
Uses Rose Pine accent colors (iris for highlights, foam for links).

### Footer
Links to docs, GitHub, license (GPL-3.0).

## Docs Site

### Sidebar Structure

```
Getting Started
  ├── Installation          # deb, rpm, AUR, Homebrew, cargo install, from source
  └── Quickstart            # First run, adding a friend, sending a message

Guides
  ├── Theming               # Built-in themes, TOML config, custom colors
  └── Friend Codes          # How friend codes work, adding/managing friends

Architecture
  ├── Overview              # High-level: P2P model, data flow, key components
  ├── Signal Protocol       # X3DH, Double Ratchet, session establishment
  └── Tor Integration       # Arti, onion services, connection pooling

Reference
  ├── FAQ                   # Common questions, troubleshooting
  └── Contributing          # How to build, test, submit PRs
```

### Content Sources

| Page | Source |
|---|---|
| Installation | README "Quick Start" + packaging sections |
| Quickstart | README "How It Works" + new walkthrough content |
| Theming | README themes section + `src/ui/theme.rs` details |
| Friend Codes | README friend codes section + protocol docs |
| Architecture Overview | CLAUDE.md architecture section |
| Signal Protocol | `docs/plans/` design docs + `src/crypto/signal.rs` |
| Tor Integration | `docs/plans/` Tor design docs + `src/tor/` |
| FAQ | New content based on common questions |
| Contributing | New content: build, test, PR workflow |

### Starlight Features

- **Search**: Pagefind (ships with Starlight, zero config)
- **Edit links**: "Edit this page on GitHub" links on every docs page
- **Prev/Next**: Automatic page navigation
- **Mobile**: Responsive sidebar with hamburger menu

## Rose Pine Theme

Map Rose Pine color tokens to Starlight's CSS custom properties:

| Starlight Token | Rose Pine Color | Hex | Usage |
|---|---|---|---|
| `--sl-color-bg` | Base | `#191724` | Page background |
| `--sl-color-bg-sidebar` | Surface | `#1f1d2e` | Sidebar background |
| `--sl-color-bg-nav` | Overlay | `#26233a` | Top nav, code blocks |
| `--sl-color-text` | Text | `#e0def4` | Body text |
| `--sl-color-text-accent` | Iris | `#c4a7e7` | Links, headings |
| Accent high | Love | `#eb6f92` | Hover states, highlights |
| Accent low | Foam | `#9ccfd8` | Secondary accents |
| Muted | Muted | `#6e6a86` | Subtle text, borders |
| Highlight low | Highlight Low | `#21202e` | Selected states |

Additional styling:
- Code blocks: Overlay (`#26233a`) background with Rose Pine syntax theme
- Logo: Subtle glow effect on hover using iris color
- Borders: Muted (`#6e6a86`) for separators

## Netlify Configuration

```toml
[build]
  base = "website/"
  command = "npm run build"
  publish = "dist/"

[build.environment]
  NODE_VERSION = "20"
```

- Netlify auto-detects Astro and serves static output
- No SSR — pure static site generation
- Domain managed in Netlify dashboard (subdomain now, custom domain later)
- Build scoped to `website/` directory via `base` setting
- `site` field in `astro.config.mjs` updated once when custom domain is configured

## Future Considerations

- **Custom domain**: Add domain in Netlify dashboard, update DNS, update `site` in `astro.config.mjs`. Netlify auto-provisions SSL.
- **Screenshots/demos**: Could add terminal screenshots or asciinema recordings to the landing page later.
- **Blog**: Astro supports blog collections if release notes or project updates are desired.
