# Chattor Website Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Build a chattor project website with Astro Starlight — a Rose Pine-themed landing page plus full documentation site, deployed on Netlify.

**Architecture:** Astro Starlight generates a static docs site from Markdown/MDX files. A splash-template index page serves as the landing page with hero + features grid. Rose Pine colors are applied via CSS custom property overrides. Netlify serves the static build from the `website/` subdirectory.

**Tech Stack:** Astro 5+, @astrojs/starlight, CSS custom properties, Netlify, Node 20

**Design doc:** `docs/plans/2026-02-19-website-design.md`

---

## Task 1: Scaffold Astro Starlight Project

**Files:**
- Create: `website/package.json`
- Create: `website/astro.config.mjs`
- Create: `website/tsconfig.json`
- Create: `website/src/content.config.ts`
- Create: `website/src/content/docs/index.mdx` (placeholder)
- Copy: `assets/chattor-logo.png` → `website/src/assets/chattor-logo.png`

**Step 1: Initialize the Astro project**

```bash
cd /home/john/chattor/chattor
mkdir -p website
cd website
npm create astro@latest -- --template starlight --no-install --no-git .
```

If the interactive scaffold fails or overwrites are needed, create manually:

```json
// website/package.json
{
  "name": "chattor-website",
  "type": "module",
  "version": "0.0.1",
  "scripts": {
    "dev": "astro dev",
    "build": "astro build",
    "preview": "astro preview"
  },
  "dependencies": {
    "astro": "^5",
    "@astrojs/starlight": "^0.34"
  }
}
```

**Step 2: Configure Starlight**

```javascript
// website/astro.config.mjs
import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

export default defineConfig({
  integrations: [
    starlight({
      title: 'chattor',
      logo: {
        src: './src/assets/chattor-logo.png',
        replacesTitle: true,
      },
      social: [
        { icon: 'github', label: 'GitHub', href: 'https://github.com/jmsingleton/chattor' },
      ],
      editLink: {
        baseUrl: 'https://github.com/jmsingleton/chattor/edit/main/website/',
      },
      customCss: [
        './src/styles/custom.css',
      ],
      sidebar: [
        {
          label: 'Getting Started',
          items: [
            'getting-started/installation',
            'getting-started/quickstart',
          ],
        },
        {
          label: 'Guides',
          items: [
            'guides/theming',
            'guides/friend-codes',
          ],
        },
        {
          label: 'Architecture',
          items: [
            'architecture/overview',
            'architecture/signal-protocol',
            'architecture/tor-integration',
          ],
        },
        {
          label: 'Reference',
          items: [
            'faq',
            'contributing',
          ],
        },
      ],
    }),
  ],
});
```

**Step 3: Create content config**

```typescript
// website/src/content.config.ts
import { defineCollection } from 'astro:content';
import { docsLoader } from '@astrojs/starlight/loaders';
import { docsSchema } from '@astrojs/starlight/schema';

export const collections = {
  docs: defineCollection({
    loader: docsLoader(),
    schema: docsSchema(),
  }),
};
```

**Step 4: Create tsconfig.json**

```json
// website/tsconfig.json
{
  "extends": "astro/tsconfigs/strict"
}
```

**Step 5: Create placeholder index**

```mdx
---
# website/src/content/docs/index.mdx
title: chattor
description: Peer-to-peer encrypted chat over Tor
template: splash
hero:
  title: chattor
  tagline: Peer-to-peer encrypted chat over Tor, right in your terminal.
  image:
    file: ../../assets/chattor-logo.png
  actions:
    - text: Get Started
      link: /getting-started/installation/
      icon: right-arrow
      variant: primary
    - text: View on GitHub
      link: https://github.com/jmsingleton/chattor
      variant: secondary
---

Placeholder — features section will be added in Task 3.
```

**Step 6: Copy logo**

```bash
cp assets/chattor-logo.png website/src/assets/chattor-logo.png
```

**Step 7: Create placeholder custom CSS**

```css
/* website/src/styles/custom.css */
/* Rose Pine theme — will be populated in Task 2 */
```

**Step 8: Install dependencies and verify build**

```bash
cd /home/john/chattor/chattor/website
npm install
npm run build
```

Expected: Build succeeds, static output in `website/dist/`.

**Step 9: Commit**

```bash
git add website/
git commit -m "feat(website): scaffold Astro Starlight project with placeholder index"
```

---

## Task 2: Apply Rose Pine Theme

**Files:**
- Modify: `website/src/styles/custom.css`

**Step 1: Write Rose Pine CSS overrides**

Reference: https://rosepinetheme.com/palette/ingredients

```css
/* website/src/styles/custom.css */

:root {
  /* Rose Pine — Base palette */
  --sl-color-white: #e0def4;       /* Text */
  --sl-color-gray-1: #e0def4;      /* Text */
  --sl-color-gray-2: #908caa;      /* Subtle */
  --sl-color-gray-3: #6e6a86;      /* Muted */
  --sl-color-gray-4: #524f67;      /* Highlight High */
  --sl-color-gray-5: #26233a;      /* Overlay */
  --sl-color-gray-6: #1f1d2e;      /* Surface */
  --sl-color-black: #191724;       /* Base */

  /* Accent — Iris */
  --sl-color-accent-low: #2a2540;  /* Iris dimmed */
  --sl-color-accent: #c4a7e7;      /* Iris */
  --sl-color-accent-high: #d8c7f0; /* Iris bright */

  /* Named colors using Rose Pine palette */
  --sl-hue-orange: 35;     /* Gold #f6c177 */
  --sl-hue-green: 168;     /* Foam #9ccfd8 */
  --sl-hue-blue: 189;      /* Foam alt */
  --sl-hue-purple: 267;    /* Iris #c4a7e7 */
  --sl-hue-red: 343;       /* Love #eb6f92 */
}

/* Logo glow on hover */
[data-logo] img:hover {
  filter: drop-shadow(0 0 8px #c4a7e780);
  transition: filter 0.2s ease;
}

/* Code blocks — Overlay background */
pre {
  background-color: #26233a !important;
}

/* Inline code */
:not(pre) > code {
  background-color: #26233a;
  color: #e0def4;
}

/* Hero section styling */
.hero {
  background: radial-gradient(ellipse at center, #1f1d2e 0%, #191724 70%);
}

/* Links — Foam for external, Iris for internal (handled by accent) */
a:hover {
  color: #9ccfd8;
}
```

**Step 2: Verify build**

```bash
cd /home/john/chattor/chattor/website && npm run build
```

Expected: Build succeeds.

**Step 3: Visual check**

```bash
cd /home/john/chattor/chattor/website && npm run preview
```

Open http://localhost:4321 — verify Rose Pine colors: dark background (#191724), purple accent links, overlay code blocks. Kill preview after checking.

**Step 4: Commit**

```bash
git add website/src/styles/custom.css
git commit -m "feat(website): apply Rose Pine dark theme via CSS custom properties"
```

---

## Task 3: Build Landing Page

**Files:**
- Modify: `website/src/content/docs/index.mdx`
- Create: `website/src/components/FeatureCard.astro`
- Create: `website/src/components/Features.astro`

**Step 1: Create the FeatureCard component**

```astro
---
// website/src/components/FeatureCard.astro
interface Props {
  icon: string;
  title: string;
  description: string;
}

const { icon, title, description } = Astro.props;
---

<div class="feature-card">
  <span class="feature-icon">{icon}</span>
  <h3>{title}</h3>
  <p>{description}</p>
</div>

<style>
  .feature-card {
    background: #1f1d2e;
    border: 1px solid #6e6a86;
    border-radius: 8px;
    padding: 1.5rem;
    transition: border-color 0.2s ease, box-shadow 0.2s ease;
  }

  .feature-card:hover {
    border-color: #c4a7e7;
    box-shadow: 0 0 12px #c4a7e720;
  }

  .feature-icon {
    font-size: 1.5rem;
    display: block;
    margin-bottom: 0.5rem;
  }

  h3 {
    color: #e0def4;
    font-size: 1.1rem;
    margin: 0 0 0.5rem 0;
  }

  p {
    color: #908caa;
    font-size: 0.9rem;
    margin: 0;
    line-height: 1.5;
  }
</style>
```

**Step 2: Create the Features grid component**

```astro
---
// website/src/components/Features.astro
import FeatureCard from './FeatureCard.astro';
---

<div class="features-section">
  <div class="features-grid">
    <FeatureCard
      icon="🔐"
      title="Signal Protocol E2E"
      description="X3DH key exchange + ChaCha20-Poly1305 encryption. No plaintext fallback — no session, no message."
    />
    <FeatureCard
      icon="🧅"
      title="Pure P2P over Tor"
      description="Each user hosts their own onion service via embedded arti. No central relay, no NAT traversal."
    />
    <FeatureCard
      icon="🗄️"
      title="Encrypted at Rest"
      description="SQLCipher database with full-text search. Your messages never touch disk unencrypted."
    />
    <FeatureCard
      icon="📡"
      title="Broadcast Channels"
      description="Ed25519-signed posts with pull-based sync. Public and friends-only channels with read receipts."
    />
    <FeatureCard
      icon="🎨"
      title="7 Terminal Themes"
      description="Dark, light, cyberpunk, minimal, and three Rose Pine variants. Fully customizable via TOML."
    />
    <FeatureCard
      icon="📬"
      title="Offline Delivery"
      description="Messages queue automatically with exponential backoff retries until your peer comes online."
    />
  </div>
</div>

<style>
  .features-section {
    max-width: 72rem;
    margin: 2rem auto;
    padding: 0 1rem;
  }

  .features-grid {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 1.25rem;
  }

  @media (max-width: 768px) {
    .features-grid {
      grid-template-columns: 1fr;
    }
  }

  @media (min-width: 769px) and (max-width: 1024px) {
    .features-grid {
      grid-template-columns: repeat(2, 1fr);
    }
  }
</style>
```

**Step 3: Create the architecture diagram component**

```astro
---
// website/src/components/ArchDiagram.astro
---

<div class="arch-section">
  <h2>How It Works</h2>
  <div class="arch-diagram">
    <div class="arch-step">
      <span class="arch-label">You</span>
    </div>
    <span class="arch-arrow">→</span>
    <div class="arch-step">
      <span class="arch-label">TUI</span>
      <span class="arch-sub">ratatui</span>
    </div>
    <span class="arch-arrow">→</span>
    <div class="arch-step highlight">
      <span class="arch-label">Signal Protocol</span>
      <span class="arch-sub">E2E encryption</span>
    </div>
    <span class="arch-arrow">→</span>
    <div class="arch-step highlight">
      <span class="arch-label">Tor</span>
      <span class="arch-sub">onion service</span>
    </div>
    <span class="arch-arrow">→</span>
    <div class="arch-step">
      <span class="arch-label">Peer</span>
    </div>
  </div>
  <p class="arch-caption">No central servers. No accounts. No metadata leakage.</p>
</div>

<style>
  .arch-section {
    text-align: center;
    margin: 3rem auto;
    max-width: 72rem;
    padding: 0 1rem;
  }

  .arch-section h2 {
    color: #e0def4;
    margin-bottom: 1.5rem;
  }

  .arch-diagram {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 0.5rem;
    flex-wrap: wrap;
  }

  .arch-step {
    background: #1f1d2e;
    border: 1px solid #6e6a86;
    border-radius: 8px;
    padding: 0.75rem 1.25rem;
    text-align: center;
  }

  .arch-step.highlight {
    border-color: #c4a7e7;
    box-shadow: 0 0 8px #c4a7e720;
  }

  .arch-label {
    display: block;
    color: #e0def4;
    font-weight: 600;
    font-size: 0.95rem;
  }

  .arch-sub {
    display: block;
    color: #908caa;
    font-size: 0.75rem;
    margin-top: 0.25rem;
  }

  .arch-arrow {
    color: #c4a7e7;
    font-size: 1.25rem;
    font-weight: bold;
  }

  .arch-caption {
    color: #6e6a86;
    font-style: italic;
    margin-top: 1rem;
  }

  @media (max-width: 600px) {
    .arch-diagram {
      flex-direction: column;
    }
    .arch-arrow {
      transform: rotate(90deg);
    }
  }
</style>
```

**Step 4: Update the landing page**

```mdx
---
title: chattor
description: Peer-to-peer encrypted chat over Tor, right in your terminal.
template: splash
hero:
  title: chattor
  tagline: Peer-to-peer encrypted chat over Tor, right in your terminal.
  image:
    file: ../../assets/chattor-logo.png
  actions:
    - text: Get Started
      link: /getting-started/installation/
      icon: right-arrow
      variant: primary
    - text: View on GitHub
      link: https://github.com/jmsingleton/chattor
      variant: secondary
---

import Features from '../../components/Features.astro';
import ArchDiagram from '../../components/ArchDiagram.astro';

<Features />

<ArchDiagram />
```

**Step 5: Verify build**

```bash
cd /home/john/chattor/chattor/website && npm run build
```

Expected: Build succeeds.

**Step 6: Commit**

```bash
git add website/src/components/ website/src/content/docs/index.mdx
git commit -m "feat(website): add landing page with features grid and architecture diagram"
```

---

## Task 4: Getting Started Docs

**Files:**
- Create: `website/src/content/docs/getting-started/installation.md`
- Create: `website/src/content/docs/getting-started/quickstart.md`

Content is adapted from the README's Quick Start section and CLAUDE.md.

**Step 1: Write installation page**

```markdown
---
title: Installation
description: How to install chattor on your system
---

## From Source (Recommended)

chattor is built with Rust. You'll need [Rust 1.70+](https://rustup.rs/) installed.

\`\`\`bash
git clone https://github.com/jmsingleton/chattor.git
cd chattor
cargo build --release
\`\`\`

The binary will be at `target/release/chattor`.

## Package Managers

### Arch Linux (AUR)

\`\`\`bash
# Binary release
yay -S chattor-bin

# Build from source
yay -S chattor-git
\`\`\`

### Debian / Ubuntu

Download the `.deb` from the [latest release](https://github.com/jmsingleton/chattor/releases):

\`\`\`bash
sudo dpkg -i chattor_*.deb
\`\`\`

### Fedora / RHEL

Download the `.rpm` from the [latest release](https://github.com/jmsingleton/chattor/releases):

\`\`\`bash
sudo rpm -i chattor-*.rpm
\`\`\`

### Homebrew (macOS)

\`\`\`bash
brew tap jmsingleton/chattor
brew install chattor
\`\`\`

## Requirements

- **Platform**: Linux, macOS, BSD (no Windows support)
- **Dependencies**: None — SQLCipher is bundled via `rusqlite`, Tor is embedded via `arti`
- **Rust**: 1.70+ (edition 2021) if building from source
```

**Step 2: Write quickstart page**

```markdown
---
title: Quickstart
description: Get up and running with chattor in 5 minutes
---

## First Run

Launch chattor:

\`\`\`bash
chattor
\`\`\`

Or with a theme:

\`\`\`bash
chattor --theme rose-pine
\`\`\`

On first run, chattor will:

1. Generate your **Ed25519 identity keypair**
2. Create an encrypted **SQLCipher database**
3. Bootstrap an embedded **Tor connection** via arti
4. Launch your **Tor hidden service** (your .onion address)

The bootstrap takes 30-60 seconds while Tor establishes circuits. You'll see an animated loading screen.

## CLI Options

| Flag | Short | Description |
|------|-------|-------------|
| `--debug` | `-d` | Enable debug logging |
| `--theme <name>` | `-t` | Theme preset (see [Theming](/guides/theming/)) |
| `--config-dir <path>` | `-c` | Custom config directory |

## Adding a Friend

1. Press **`[a]`** to open the Add Friend modal
2. Enter your friend's **32-word friend code** (they can find it by pressing **`[i]`** for Identity)
3. A friend request is sent over Tor
4. Once they accept, you can exchange messages

## Your Identity

Press **`[i]`** to view your identity:

- Your **.onion address** — this is your network identity
- Your **friend code** — share this with people who want to reach you
- Press **`[o]`** to copy your onion address, **`[c]`** to copy your friend code

## Sending Messages

1. Select a friend in the sidebar (arrow keys or mouse)
2. Type your message in the input bar
3. Press **Enter** to send

Messages are encrypted with Signal Protocol before leaving your machine. If your friend is offline, messages are queued and delivered automatically when they come back online.

## Keybindings

| Key | Action |
|-----|--------|
| `q` | Quit |
| `a` | Add friend |
| `i` | View identity |
| `r` | View friend requests |
| `n` | Toggle notifications |
| `Tab` | Switch between sidebar and chat |
| `↑/↓` | Navigate friends/messages |
```

**Step 3: Verify build**

```bash
cd /home/john/chattor/chattor/website && npm run build
```

Expected: Build succeeds, new pages at `/getting-started/installation/` and `/getting-started/quickstart/`.

**Step 4: Commit**

```bash
git add website/src/content/docs/getting-started/
git commit -m "docs(website): add installation and quickstart pages"
```

---

## Task 5: Guides — Theming and Friend Codes

**Files:**
- Create: `website/src/content/docs/guides/theming.md`
- Create: `website/src/content/docs/guides/friend-codes.md`

**Step 1: Write theming guide**

Content adapted from README themes section and `src/ui/theme.rs`.

```markdown
---
title: Theming
description: Customize chattor's appearance with built-in themes or your own colors
---

## Built-in Themes

chattor ships with 7 themes:

| Theme | Description |
|-------|-------------|
| `dark` | Default dark theme |
| `light` | Light background |
| `cyberpunk` | Neon accents on dark |
| `minimal` | Subdued, low-contrast |
| `rose-pine` | Rosé Pine dark |
| `rose-pine-moon` | Rosé Pine Moon (darker) |
| `rose-pine-dawn` | Rosé Pine Dawn (light) |

Switch themes via CLI:

\`\`\`bash
chattor --theme cyberpunk
\`\`\`

## Custom Theme via TOML

Create `~/.config/chattor/theme.toml` to override any color:

\`\`\`toml
# Start from a preset
name = "my-custom"

# Override specific colors (hex format)
bg = "#1a1b26"
fg = "#c0caf5"
accent = "#7aa2f7"
border = "#3b4261"
border_focused = "#7aa2f7"

# Component-specific overrides
sidebar_selected_fg = "#7aa2f7"
sidebar_unread = "#e0af68"
msg_own_sender = "#9ece6a"
msg_peer_sender = "#7aa2f7"
error = "#f7768e"
success = "#9ece6a"
\`\`\`

## Available Color Fields

Every field in the theme is overridable:

**Global:** `bg`, `fg`, `fg_dim`, `accent`, `border`, `border_focused`

**Header:** `header_fg`, `header_accent`

**Sidebar:** `sidebar_selected_fg`, `sidebar_unread`, `sidebar_status_online`, `sidebar_channel_header`

**Conversation:** `msg_own_sender`, `msg_peer_sender`, `msg_timestamp`, `msg_status_sent`, `msg_status_delivered`, `msg_status_read`, `msg_status_failed`, `msg_ephemeral`

**Input:** `input_fg`, `input_placeholder`

**Modals:** `modal_border`, `modal_title`, `error`, `warning`, `success`

**Channels:** `channel_border`, `channel_read_count`

## Config Paths

| Platform | Path |
|----------|------|
| **Linux** | `~/.config/chattor/theme.toml` |
| **macOS** | `~/Library/Application Support/chattor/theme.toml` |
```

**Step 2: Write friend codes guide**

```markdown
---
title: Friend Codes
description: How chattor's 32-word friend codes work
---

## What Are Friend Codes?

Friend codes are chattor's way of exchanging identities without a central server. Each code is a **32-word mnemonic** that encodes your public key, similar to how Bitcoin seed phrases work.

A friend code looks like this:

\`\`\`
alpine bridge castle dragon ember falcon glacier harbor
island jasper kindle lantern meadow nebula orchid prism
quartz river sunset tower umbra velvet whisper xenon
\`\`\`

(8 groups of 4 words, drawn from a 256-word dictionary)

## How They Work

1. Your Ed25519 public key is encoded as a sequence of 32 words from a fixed dictionary
2. The encoding is deterministic — the same key always produces the same code
3. Friend codes map to `.onion` addresses via a SHA-256 hash
4. The mapping is one-way: you can derive the `.onion` from the code, but not the code from the `.onion`

## Sharing Your Code

1. Press **`[i]`** to open the Identity modal
2. Press **`[c]`** to copy your friend code to the clipboard
3. Share it via any out-of-band channel (in person, encrypted message, etc.)

## Adding a Friend

1. Press **`[a]`** to open the Add Friend modal
2. Paste or type your friend's 32-word code
3. A friend request is sent over Tor to their hidden service
4. Once they accept, a Signal Protocol session is established via X3DH key exchange

## Security Considerations

- Friend codes should be exchanged over a **trusted channel** — they are your identity
- Anyone with your friend code can send you a friend request
- You can reject unwanted friend requests
- The friend code itself does not reveal your `.onion` address to a passive observer (SHA-256 is one-way)
```

**Step 3: Verify build**

```bash
cd /home/john/chattor/chattor/website && npm run build
```

**Step 4: Commit**

```bash
git add website/src/content/docs/guides/
git commit -m "docs(website): add theming and friend codes guides"
```

---

## Task 6: Architecture Docs

**Files:**
- Create: `website/src/content/docs/architecture/overview.md`
- Create: `website/src/content/docs/architecture/signal-protocol.md`
- Create: `website/src/content/docs/architecture/tor-integration.md`

Content adapted from CLAUDE.md architecture section and design docs.

**Step 1: Write architecture overview**

```markdown
---
title: Architecture Overview
description: How chattor's components fit together
---

## Design Principles

- **Privacy-first**: No central servers, no telemetry, no metadata leakage
- **Encrypted everywhere**: at-rest (SQLCipher), in-transit (Tor), end-to-end (Signal Protocol)
- **Pure P2P**: Each user = one Tor hidden service
- **Unix-focused**: Linux, macOS, BSD

## High-Level Data Flow

\`\`\`
User → TUI (ratatui) → Signal Protocol (E2E) → Tor Hidden Service → Peer
\`\`\`

When you send a message:

1. You type in the TUI input bar
2. The message is encrypted with **Signal Protocol** (ChaCha20-Poly1305)
3. The ciphertext is wrapped in a JSON envelope with type metadata
4. It's sent via the **connection pool** (reuses cached Tor circuits)
5. If the peer is offline, it's queued in the **message queue** with retries
6. The peer's Tor hidden service receives the envelope
7. The peer decrypts with their Signal Protocol session
8. The plaintext is stored in the encrypted SQLCipher database

## Key Components

### Application State (`src/app.rs`)

The central `App` struct owns all runtime state: settings, database, identity keys, Tor client, hidden service, and message queue. `App::new()` initializes synchronously; Tor bootstrapping happens asynchronously via `init_tor()`.

### Database Layer (`src/db/`)

SQLCipher provides at-rest encryption. The schema (currently v8) includes tables for friends, conversations, messages, signal sessions, blocked users, channels, and a key-value settings store. FTS5 virtual tables enable full-text message search with auto-sync triggers.

Migrations run automatically from v2 through v8 on startup.

### Networking (`src/net/`)

- **Connection Pool** (`pool.rs`): Caches Tor circuits per peer. Idle connections evicted after 5 minutes. Retry-on-stale: dead connections are evicted and retried with a fresh circuit.
- **Message Queue** (`queue.rs`): FIFO queue persisted in the database. Exponential backoff retries (30s base, doubles each attempt, capped at 15min, 24h expiry).
- **Framing** (`framing.rs`): Length-prefixed TCP framing for message I/O over Tor streams.

### Protocol (`src/protocol/`)

13 message types: `FriendRequest`, `FriendRequestAccept`, `FriendRequestReject`, `TextMessage`, `DeliveryReceipt`, `ReadReceipt`, `ChannelSubscribe`, `ChannelUnsubscribe`, `ChannelPost`, `ChannelSyncRequest`, `ChannelSyncResponse`, `ChannelPostReceipt`, `Presence`.

All messages are JSON-serialized. Encrypted messages use a Signal Protocol envelope with base64-encoded ciphertext and a type flag (PreKeyMessage or Message).

### UI (`src/ui/`)

Built with ratatui. Layout: friends sidebar (left) + conversation/channel view (right). Modals for add friend, friend requests, identity, settings, and channel subscribe. Theme engine with 7 presets and TOML config override.

## Project Structure

\`\`\`
src/
├── app.rs              # Application state
├── cli.rs              # CLI parsing (clap)
├── main.rs             # Entry point, event loop
├── crypto/             # Ed25519 identity, Signal Protocol
├── db/                 # SQLCipher database, queries, schema
├── net/                # Connection pool, message queue, framing
├── protocol/           # Friend codes, messages, friend requests
├── tor/                # Arti client, onion service, connections
└── ui/                 # TUI layout, themes, modals
\`\`\`
```

**Step 2: Write Signal Protocol page**

```markdown
---
title: Signal Protocol
description: How chattor implements end-to-end encryption
---

## Overview

chattor implements Signal Protocol using pure Rust crates — not a binding to libsignal-c. The implementation uses:

- **x25519-dalek** for X3DH (Extended Triple Diffie-Hellman) key exchange
- **chacha20poly1305** for authenticated encryption (AEAD)
- **ed25519-dalek** for identity keys and message signing

## X3DH Key Exchange

When two users establish a conversation, they perform an X3DH handshake to derive a shared secret:

### Session Establishment Flow

1. **Alice sends a friend request** to Bob (Ed25519-signed)
2. **Bob accepts**: generates a PreKeyBundle (identity key + signed prekey + one-time prekey), stores the private material, and queues the accept message
3. **Alice receives the accept**: calls `from_prekey_bundle_real()` with Bob's bundle, derives the shared secret, stores the session, and queues a handshake PreKey message
4. **Bob receives the handshake**: loads stored private material, calls `from_prekey_message_real()` to derive the same shared secret, stores the session, and cleans up private material
5. **Both sides now have established sessions** — bidirectional messaging works

### Key Types

| Key | Purpose |
|-----|---------|
| Identity Key (Ed25519) | Long-term identity, signs prekeys |
| Signed PreKey (X25519) | Medium-term key in PreKeyBundle |
| One-Time PreKey (X25519) | Ephemeral key for forward secrecy |
| Ephemeral Key (X25519) | Per-handshake key from initiator |

## Message Encryption

Once a session is established, messages are encrypted with **ChaCha20-Poly1305**:

1. Plaintext is serialized to bytes
2. A unique nonce is generated per message
3. The message is encrypted with the session's chain key
4. The ciphertext + nonce are base64-encoded into a JSON envelope
5. The envelope includes a `signal_type` field: `PreKeyMessage` (first message) or `Message` (subsequent)

## Security Properties

- **Forward secrecy**: Compromising long-term keys doesn't decrypt past messages
- **No plaintext fallback**: If no Signal session exists, encryption fails hard — messages are never sent unencrypted
- **Session persistence**: Signal sessions are stored in the encrypted SQLCipher database
- **Key verification**: Friend requests are Ed25519-signed to prevent impersonation
```

**Step 3: Write Tor integration page**

```markdown
---
title: Tor Integration
description: How chattor uses embedded Tor for anonymous P2P networking
---

## Embedded Tor via Arti

chattor embeds Tor using [arti](https://gitlab.torproject.org/tpo/core/arti), the Tor Project's pure-Rust Tor implementation. This means:

- **No system Tor required** — arti is compiled into the chattor binary
- **Persistent identity** — your `.onion` address survives restarts (arti manages the key in its state directory)
- **Single process** — no separate Tor daemon to manage

## Onion Service

Each chattor user runs a v3 Tor onion service. This is your "server" — other users connect to your `.onion` address to send you messages.

### Lifecycle

1. On startup, `TorClient::new_with_data_dir()` bootstraps the Tor connection using a persistent state directory
2. `HiddenService::launch()` starts the onion service via arti's `launch_onion_service()`
3. The `.onion` address is cached in the database's `app_settings` table for display before Tor connects
4. A listener task accepts incoming Tor rendezvous streams and processes messages

### Address Mapping

Friend codes map to `.onion` addresses via SHA-256:

\`\`\`
friend_code → SHA-256 hash → .onion address
\`\`\`

This mapping is deterministic (same code always produces the same address) and one-way (you can't reverse the `.onion` back to the friend code).

## Connection Pool

Building a Tor circuit takes 10-30 seconds. To avoid this per-message latency, chattor maintains a **connection pool** (`src/net/pool.rs`):

- **Per-peer caching**: One cached circuit per peer
- **Idle eviction**: Unused connections are dropped after 5 minutes
- **Retry-on-stale**: If a cached circuit is dead, it's evicted and a fresh one is built
- **Timeouts**: 30s for circuit building, 10s for message sending

## Message Delivery

Messages flow through the connection pool:

1. Check pool for an existing circuit to the peer
2. If found, send the message through the cached circuit
3. If the circuit is dead, evict it and build a new one (retry once)
4. If the peer is unreachable, the message is queued in the offline queue

The **offline message queue** (`src/net/queue.rs`) handles delivery retries:

- FIFO queue persisted in the database
- Exponential backoff: 30s base, doubles each attempt, capped at 15 minutes
- 24-hour expiry window — messages older than 24h are dropped
- Up to 10 peers are processed concurrently via a semaphore
```

**Step 4: Verify build**

```bash
cd /home/john/chattor/chattor/website && npm run build
```

**Step 5: Commit**

```bash
git add website/src/content/docs/architecture/
git commit -m "docs(website): add architecture overview, Signal Protocol, and Tor integration pages"
```

---

## Task 7: Reference Docs — FAQ and Contributing

**Files:**
- Create: `website/src/content/docs/faq.md`
- Create: `website/src/content/docs/contributing.md`

**Step 1: Write FAQ**

```markdown
---
title: FAQ
description: Frequently asked questions about chattor
---

## General

### What is chattor?

A peer-to-peer encrypted chat application that runs in your terminal. Each user hosts their own Tor hidden service — there are no central servers, accounts, or registration.

### What platforms are supported?

Linux, macOS, and BSD. Windows is not supported.

### Does chattor require a system Tor installation?

No. Tor is embedded via the [arti](https://gitlab.torproject.org/tpo/core/arti) library (pure Rust). Everything is bundled in the chattor binary.

### Is chattor anonymous?

chattor routes all traffic through Tor, which hides your IP address from peers. Your identity is your Ed25519 keypair and `.onion` address — no real-world identity is attached.

## Security

### What encryption does chattor use?

Three layers:

1. **End-to-end**: Signal Protocol (X3DH + ChaCha20-Poly1305) between peers
2. **In-transit**: Tor encryption for network transport
3. **At-rest**: SQLCipher for the local database

### Can messages be sent unencrypted?

No. The plaintext fallback was removed entirely. If no Signal Protocol session exists with a peer, messages cannot be sent.

### Are messages stored on any server?

No. Messages are only stored on the sender's and recipient's local machines, in an encrypted SQLCipher database.

## Troubleshooting

### Tor bootstrap is taking a long time

Initial Tor bootstrap can take 30-60 seconds. Subsequent startups are faster because arti caches state. If it takes more than 2 minutes, check your network connection.

### Messages aren't being delivered

If a peer is offline, messages are automatically queued and retried with exponential backoff (up to 24 hours). Check that both peers have active Tor connections.

### How do I reset my identity?

Delete the chattor data directory:

\`\`\`bash
# Linux
rm -rf ~/.local/share/chattor/

# macOS
rm -rf ~/Library/Application\\ Support/chattor/
\`\`\`

This deletes your identity, all messages, and all friend relationships. There is no recovery.
```

**Step 2: Write contributing guide**

```markdown
---
title: Contributing
description: How to build, test, and contribute to chattor
---

## Building from Source

\`\`\`bash
git clone https://github.com/jmsingleton/chattor.git
cd chattor
cargo build
\`\`\`

## Running Tests

\`\`\`bash
# All tests
cargo test

# Specific module
cargo test protocol::message

# Integration tests only
cargo test --test integration

# E2E crypto/messaging tests
cargo test --test e2e_messaging

# With output
cargo test -- --nocapture
\`\`\`

## Code Quality

\`\`\`bash
# Format
cargo fmt

# Lint
cargo clippy -- -D warnings
\`\`\`

## Project Structure

See the [Architecture Overview](/architecture/overview/) for a guide to the codebase.

## Testing Strategy

- **Unit tests**: Per-module in `#[cfg(test)]` blocks
- **Integration tests**: `tests/integration/` for cross-module interaction
- **E2E tests**: `tests/e2e_messaging.rs` for full Signal Protocol pipeline
- **Database tests**: Use `tempfile` crate for isolated test databases

## Submitting Changes

1. Fork the repository
2. Create a feature branch from `main`
3. Make your changes with tests
4. Run `cargo fmt` and `cargo clippy -- -D warnings`
5. Run `cargo test` and ensure all tests pass
6. Submit a pull request

## License

chattor is dual-licensed under MIT and Apache-2.0.
```

**Step 3: Verify build**

```bash
cd /home/john/chattor/chattor/website && npm run build
```

**Step 4: Commit**

```bash
git add website/src/content/docs/faq.md website/src/content/docs/contributing.md
git commit -m "docs(website): add FAQ and contributing pages"
```

---

## Task 8: Netlify Configuration

**Files:**
- Create: `website/netlify.toml`
- Create: `website/.gitignore`

**Step 1: Write Netlify config**

```toml
# website/netlify.toml
[build]
  command = "npm run build"
  publish = "dist/"

[build.environment]
  NODE_VERSION = "20"

# Cache node_modules between builds
[build.processing]
  skip_processing = false

# Redirect /docs to /getting-started/installation/ for convenience
[[redirects]]
  from = "/docs"
  to = "/getting-started/installation/"
  status = 301

# Security headers
[[headers]]
  for = "/*"
  [headers.values]
    X-Frame-Options = "DENY"
    X-Content-Type-Options = "nosniff"
    Referrer-Policy = "strict-origin-when-cross-origin"
```

Note: The Netlify `base` directory is set in the Netlify dashboard (or `netlify.toml` at repo root), not inside the `website/netlify.toml`. When connecting the repo to Netlify, set the base directory to `website/`.

**Step 2: Write .gitignore**

```
# website/.gitignore
node_modules/
dist/
.astro/
```

**Step 3: Verify build one final time**

```bash
cd /home/john/chattor/chattor/website && npm run build
```

Expected: Clean build, all pages generated.

**Step 4: Commit**

```bash
git add website/netlify.toml website/.gitignore
git commit -m "chore(website): add Netlify config and gitignore"
```

---

## Task 9: Final Verification and Cleanup

**Step 1: Full build from clean state**

```bash
cd /home/john/chattor/chattor/website
rm -rf node_modules dist .astro
npm install
npm run build
```

Expected: Clean build with no warnings or errors.

**Step 2: Preview and visual check**

```bash
cd /home/john/chattor/chattor/website && npm run preview
```

Check in browser at http://localhost:4321:

- [ ] Landing page: logo, tagline, CTA buttons, features grid, architecture diagram
- [ ] Rose Pine colors: dark background, purple accents, correct text colors
- [ ] Sidebar navigation: all 4 groups with correct pages
- [ ] Each docs page loads and renders correctly
- [ ] Search works (Pagefind)
- [ ] Mobile responsive (resize browser)
- [ ] Edit page links point to correct GitHub URLs

**Step 3: Fix any issues found**

Address any build warnings, broken links, or visual problems.

**Step 4: Final commit if fixes were needed**

```bash
git add website/
git commit -m "fix(website): address review feedback"
```

**Step 5: Verify Rust tests still pass**

```bash
cd /home/john/chattor/chattor && cargo test
```

The website directory should not affect Rust builds, but verify nothing was accidentally modified.
