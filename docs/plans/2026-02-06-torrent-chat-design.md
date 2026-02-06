# torrent-chat - Privacy-First TUI Chat Application

**Design Document**
Date: 2026-02-06
Status: Draft

## Overview

A privacy and security-first terminal user interface (TUI) chat application with beautiful modern aesthetics. Built for terminal users who value privacy, featuring end-to-end encryption, Tor hidden services, and ephemeral messaging.

## Core Principles

- Privacy-first architecture (no central servers, no telemetry)
- Beautiful, modern TUI with smooth animations
- Fast and responsive despite Tor latency
- Data encrypted at rest
- Unix-focused (Linux, macOS, BSD - no Windows support)

---

## Section 1: Core Architecture & Technology Stack

### Technology Foundation

**Language**: Rust
- Memory safety and performance
- Excellent ecosystem for networking, crypto, and TUI
- Strong type system prevents common security bugs

**TUI Framework**: ratatui
- Modern, actively maintained
- Gradient and rich color support
- Smooth animations and responsive layouts

**Networking**: Tor hidden services via `arti`
- Pure Rust Tor implementation
- Each user runs a hidden service
- No central servers required
- Strong anonymity guarantees

**Storage**: SQLCipher
- Encrypted SQLite database
- AES-256 encryption for messages at rest
- Standard SQL interface with encryption layer

**Platforms**: Linux, macOS, BSDs (no Windows)

### Network Architecture

Pure peer-to-peer over Tor. Each user runs a hidden service identified by a v3 .onion address (56 characters). No central servers, no registration, no phone numbers.

**User Identity**:
- Primary: .onion address (cryptographically derived from Ed25519 keypair)
- Human-friendly: Friend codes (e.g., `happy-tiger-7342`)
- Optional: Vanity .onion prefixes (e.g., `alice2k3b4d...onion`)

**Connection Model**:
- Peer-to-peer connections over Tor (6-hop encryption)
- Each message: 300-800ms latency (acceptable for text chat)
- Connection establishment: 2-5 seconds

### Security Model

**Defense Layers**:
1. Transport: All connections over Tor hidden services
2. End-to-end: Double Ratchet encryption per conversation
3. At-rest: SQLCipher encrypts database with master password

**Privacy Guarantees**:
- No IP address exposure (Tor hidden services)
- No metadata logging for ephemeral messages
- Friend list encrypted at rest
- No telemetry or analytics
- Ephemeral mode leaves zero disk traces

---

## Section 2: Friend Management & Discovery

### Friend Addition Flow

1. **Generate Friend Code**: User accesses "Share Friend Code" in UI
2. **Share Code**: User shares short code (e.g., `happy-tiger-7342`) via existing secure channel
3. **Add Friend**: Recipient enters code, app resolves to .onion address
4. **Send Request**: App sends friend request over Tor to recipient's hidden service
5. **Approve/Reject**: Recipient sees "Friend request from `alice2k3b...`" with accept/reject options
6. **Handshake**: On approval, both parties exchange .onion addresses and establish E2E encrypted session

### Friend Code System

**Format**: `word-number-word-number` (e.g., `happy-7834-tiger-2910`)
- Length: 12-16 characters
- Deterministic mapping to .onion address
- Includes checksum for typo detection
- Easy to share verbally, via text, or written down

**Properties**:
- Regeneratable if compromised
- Case-insensitive for ease of entry
- Uses pronounceable word list for memorability

### Vanity .onion Mining

Users can optionally "mine" a vanity .onion address with a custom prefix.

**Mining UI**:
- Dedicated screen showing mining progress
- Real-time hash rate display
- Estimated time to completion
- Visual explanation of effort vs prefix length
- Background mining (doesn't block app usage)

**Effort Estimates**:
- 5 characters: Minutes
- 6-7 characters: Hours
- 8+ characters: Days

**Default Behavior**:
- Users start with random .onion address
- Vanity mining is opt-in and clearly explained
- Mining process is beautiful and intentional (progress bars, animations)

### Online Status

**Presence Detection**:
- Connection-based (ping every 2-3 minutes)
- States: Online, Away (5+ min idle), Offline (unreachable)
- Gracefully handles Tor latency and false negatives

**Privacy Consideration**:
- Low-frequency pings minimize traffic analysis
- Users can disable status sharing in settings
- Offline mode available for maximum privacy

---

## Section 3: Messaging System

### Conversation Modes

Each 1-on-1 conversation has a mode toggle:

**Persistent Mode**:
- Messages saved to encrypted SQLCipher database
- Full history retained until manually deleted
- Searchable message history
- Standard chat experience

**Ephemeral Mode**:
- Messages kept in RAM only
- Never written to disk
- History cleared on app close
- Two sub-modes: Timer-based and Burn-after-reading

### Ephemeral Message Types

**1. Timer-Based Deletion**:
- Messages auto-delete after configurable duration
- Options: 5 minutes, 1 hour, 6 hours, 1 day, 1 week
- Countdown timer visible in UI for both parties
- Synchronized deletion (best-effort over P2P)
- Sender sets timer per-message or per-conversation

**2. Burn-After-Reading**:
- Messages delete immediately after recipient views them
- Status progression: Sent → Delivered → Read → Burned
- One-time view only
- Sender sees burned status confirmation

### Message Protocol

**Message Structure**:
- Content (encrypted payload)
- Timestamp (sender's local time)
- Message type (text, timer-ephemeral, burn-after-reading)
- Sender identity (.onion address)
- Signature (Ed25519)
- Encryption envelope (Double Ratchet)

**Message Flow**:
1. Sender composes message
2. Message encrypted with per-conversation key
3. Sent over Tor to recipient's hidden service
4. Recipient decrypts and verifies signature
5. Delivery receipt sent back (optional)
6. Read receipt sent when viewed (optional)

**Delivery Guarantees**:
- Messages queued when recipient offline
- Delivered on reconnection
- Failed delivery indicators with manual retry
- Ephemeral messages expire after 24hr in queue

### Storage

**Persistent Messages**:
- Location: `~/.config/torrent-chat/messages.db`
- Encryption: SQLCipher with AES-256
- Key derivation: Master password → Argon2id → database key
- Schema: conversations, messages, friends, channels, settings

**Ephemeral Messages**:
- In-memory data structures only
- Wiped on app exit
- No swap file persistence (memory locked if possible)
- Zero disk traces

---

## Section 4: Broadcast Channels

### Channel System

Each user automatically has two broadcast channels:

**1. Public Channel**:
- Anyone who knows your .onion/.friend code can subscribe
- For announcements, status updates, blog-style posts
- One-way communication (no replies)

**2. Friends-Only Channel**:
- Only approved friends can subscribe
- For personal updates, coordination with friend group
- Auto-available to friends (no separate subscription needed)

### Channel Mechanics

**Publishing**:
- User composes post in dedicated channel UI
- Post broadcast to all subscribers' hidden services
- Real-time delivery to online subscribers
- Offline subscribers fetch on reconnect

**Subscribing**:
- Public channels: Discovered via friend code (same mechanism as adding friends)
- Friends-only: Auto-subscribed when friend request approved
- UI prompts: "Subscribe to [username]'s channel?"

**Content Types**:
- Text posts (markdown support)
- Future: Small inline images, links

### Channel Storage

**Retention Policies**:
- Per-channel setting: Keep last N posts, or time-based retention
- Options: Last 50 posts, last 7 days, last 30 days, forever
- Ephemeral channels: Never persist posts (RAM only)

**Storage**:
- Channel posts stored in same SQLCipher database
- Schema: channels, channel_posts, subscriptions
- Posts encrypted at rest like messages

### UI Representation

**Sidebar**:
- Channels listed separately from 1-on-1 chats
- Visual distinction: 📢 icon, different color accent
- Unread post count badge
- Collapsed/expanded sections

**Channel View**:
- Chronological feed of posts
- Author name/avatar (if set)
- Timestamp
- Smooth scrolling
- No reply mechanism (one-way broadcast)

---

## Section 5: UI/UX Design

### Visual Philosophy

**Goals**:
- Modern, polished, "vibey"
- Beautiful by default, customizable for power users
- Smooth animations without sacrificing performance
- Terminal-native aesthetics (respects terminal culture)

**Inspiration**: `gitui`, `bottom`, `lazydocker` - rich colors, gradients, Unicode symbols

### Layout Structure

```
┌─────────────────────────────────────────────────────────────────┐
│  torrent-chat  ⬤ Online              [@alice2k3...]        ⚙    │
├──────────────┬──────────────────────────────────────────────────┤
│              │                                                  │
│ Friends      │         Conversation View                        │
│  ⬤ Bob       │                                                  │
│  ⬤ Carol     │    [Messages with smooth scroll]                │
│  ○ Dave      │                                                  │
│              │    Bob is typing...                              │
│ Channels     │                                                  │
│  📢 News (2) │                                                  │
│  📢 Updates  │                                                  │
│              │                                                  │
├──────────────┼──────────────────────────────────────────────────┤
│  Status Bar  │  [Message input]                   [Send] [Esc] │
└──────────────┴──────────────────────────────────────────────────┘
```

**Sidebar (Left)**:
- Friends list with online status indicators
- Channels section (collapsible)
- Unread message/post counts
- Visual distinction between DMs and channels

**Main Panel (Center/Right)**:
- Active conversation or channel feed
- Smooth scrolling with momentum
- Message bubbles with sender indicators
- Timestamps (relative or absolute, configurable)
- Typing indicators

**Status Bar (Bottom)**:
- Connection status (Tor circuit health)
- Current mode (Persistent/Ephemeral)
- Input area with character count
- Keyboard shortcuts hint

### Animations

**Message Animations**:
- Fade in on send (100ms ease-out)
- Slide in on receive (150ms ease-out)
- Smooth scroll to bottom on new message

**Status Transitions**:
- Online → Away: Color fade over 300ms
- Connecting animation: Pulsing indicator
- Typing indicator: Gentle pulse/bounce

**Panel Transitions**:
- Switching conversations: Cross-fade (200ms)
- Opening settings: Slide in from right
- Modal dialogs: Fade in backdrop, scale up dialog

**Connection States**:
- Tor bootstrap: Progress bar with percentage
- Sending message: Subtle spinner next to message
- Mining vanity address: Hash rate animation, progress ring

### Default Theme

**Color Palette**:
- Background: Deep blue-black gradient (`#0a0e27` → `#1a1e3a`)
- Primary accent: Cyan/teal gradient (`#00d4ff` → `#00ffaa`)
- Text: Soft white (`#e0e0e0`), gray for secondary (`#888888`)
- Status: Green (`#00ff88`), Amber (`#ffaa00`), Gray (`#666666`)
- Borders: Gradient with subtle glow effect
- Ephemeral mode: Purple/magenta accent (`#bb00ff`)

**Typography**:
- Clear Unicode box-drawing characters
- Nerd Font icons (optional, fallback to ASCII)
- Monospace-friendly spacing

### Theming System

**Configuration**:
- Location: `~/.config/torrent-chat/theme.toml`
- Hot-reload without restart
- Override individual colors or entire palettes

**Preset Themes**:
1. **Dark** (default) - Blue-black gradient, cyan accents
2. **Light** - Clean white/gray, blue accents
3. **Cyberpunk** - Green-on-black Matrix vibes, neon accents
4. **Minimal** - Monochrome, maximum contrast, no gradients
5. **Rosé Pine** - Main variant
6. **Rosé Pine Moon** - Darker Rosé Pine
7. **Rosé Pine Dawn** - Light Rosé Pine

**Customization Options**:
- All color values (hex codes)
- Gradient directions and stops
- Animation speeds (or disable)
- Border styles (rounded, sharp, double-line)
- Status indicator symbols

**Example theme.toml**:
```toml
[colors]
background = "#0a0e27"
background_gradient = "#1a1e3a"
primary = "#00d4ff"
accent = "#00ffaa"
text = "#e0e0e0"
text_secondary = "#888888"

[animations]
enabled = true
message_fade_ms = 100
status_transition_ms = 300

[symbols]
online = "⬤"
away = "◐"
offline = "○"
```

---

## Section 6: Security & Encryption

### Encryption Layers

**1. Transport Layer (Tor)**:
- All connections over Tor hidden services
- 6-hop onion routing (3 hops sender, 3 hops receiver)
- No IP address exposure
- Resistant to traffic analysis

**2. End-to-End Encryption (Double Ratchet)**:
- Signal Protocol's Double Ratchet algorithm
- Per-conversation encryption keys
- Forward secrecy (past messages protected if keys compromised)
- Future secrecy (keys refreshed with each message)

**3. At-Rest Encryption (SQLCipher)**:
- Database encrypted with AES-256-CBC
- PBKDF2 or Argon2id key derivation from master password
- All persistent data encrypted (messages, friends, settings)

### Key Management

**Master Password**:
- User sets on first launch
- Derives database encryption key (Argon2id, high cost)
- Never stored in plaintext
- Optional: Biometric unlock on supported platforms (future)

**Per-Friend E2E Keys**:
- Established during friend request handshake
- Initial key exchange: X25519 ECDH
- Ratchet state stored in encrypted database
- Keys rotated with each message (Double Ratchet)

**Identity Keys**:
- Ed25519 keypair per user
- Private key encrypts at rest
- Public key = .onion address (cryptographically bound)
- Used for signing messages and friend requests

### Authentication

**Friend Request Authentication**:
- Friend request includes proof-of-ownership of .onion address
- Signature over request data with identity key
- Prevents impersonation attacks

**Message Authentication**:
- All messages signed by sender's identity key
- Recipient verifies signature before displaying
- Tampering detected and rejected

**Optional Safety Numbers**:
- Like Signal's safety number verification
- Users can verify fingerprints out-of-band
- Detects MITM attacks
- UI shows "verified" badge for verified friends

### Privacy Features

**Minimal Metadata**:
- No server-side metadata (no servers!)
- Timestamps are sender's local time (not network time)
- Ephemeral messages: No timestamps stored after deletion

**Traffic Analysis Resistance**:
- Optional message padding (fixed-size packets)
- Configurable dummy traffic (future feature)
- Timing obfuscation (random delays)

**No Telemetry**:
- Zero phone-home behavior
- No crash reports unless user explicitly exports logs
- No analytics, no tracking
- Open source and auditable

### Security Considerations

**Threat Model**:
- Protects against: Mass surveillance, traffic analysis, ISP snooping, metadata collection
- Does NOT protect against: Compromised endpoints, physical device access, advanced targeted attacks

**Known Limitations**:
- Tor latency enables timing correlation attacks (mitigation: padding, dummy traffic)
- Ephemeral messages require trust (recipient could screenshot/log)
- No protection if device compromised (keylogger, memory dump)

**Security Best Practices**:
- Regular dependency audits
- Cryptography review by experts
- Reproducible builds
- Security advisories published promptly
- Bug bounty program (post-1.0)

---

## Section 7: Error Handling & Edge Cases

### Tor Connection Handling

**Bootstrap Process**:
- Show progress bar during initial Tor connection
- States: Connecting → Building circuits → Connected
- Estimate time remaining (typically 30-60 seconds)
- Clear error messages if bootstrap fails

**Connection Failures**:
- Distinguish: Friend offline vs Tor circuit failure vs network issue
- Retry logic with exponential backoff
- Manual retry button in UI
- Graceful degradation (queue messages for later)

**Circuit Timeouts**:
- Default timeout: 60 seconds for message send
- Show "connecting..." indicator during slow circuits
- Allow user to cancel long-running operations

### Message Delivery

**Offline Friends**:
- Messages queued locally when friend offline
- Delivered automatically on reconnection
- Queue persisted in encrypted database
- UI shows "queued" status next to message

**Failed Delivery**:
- Retry up to 3 times with backoff
- After retries exhausted, show "failed" indicator
- Manual retry button
- Option to cancel and delete queued message

**Ephemeral Message Expiry**:
- Queued ephemeral messages expire after 24 hours
- Prevent stale ephemeral messages from delivering days later
- User notified of expired messages

**Out-of-Order Delivery**:
- Messages timestamped by sender
- UI sorts by timestamp even if received out-of-order
- Handles clock skew gracefully (tolerate ±5 min difference)

### Database Operations

**Corruption Detection**:
- Periodic integrity checks (on startup, idle time)
- If corruption detected, show recovery UI
- Options: Restore from backup, export friends list, reset database

**Backup System**:
- Automatic encrypted backups to `~/.config/torrent-chat/backups/`
- Keep last 7 daily backups
- Manual backup/restore in settings
- Export friends list as plaintext (for disaster recovery)

**Migration**:
- Schema versioning in database
- Automatic migrations on app upgrade
- Backup before migration
- Rollback on migration failure

### Edge Cases

**Multiple Devices**:
- No cross-device sync (by design, for privacy)
- Each device = separate .onion address
- Users manually add each device as separate friend
- Future: Optional device linking (complex, privacy trade-offs)

**Friend Changes .onion**:
- Existing friend updates their .onion address
- App shows warning: "Alice's identity changed - verify out-of-band"
- Manual re-verification required
- Old messages retained but clearly marked

**Vanity Mining Interrupted**:
- Save mining progress (current nonce, target prefix)
- Resume mining on next app launch
- Option to cancel and switch to new prefix

**Clock Skew**:
- Tor users often have incorrect system time
- Tolerate ±15 minute timestamp differences
- Sort messages by received time if sender time unreliable
- Show warning if clock significantly off

**Low Bandwidth**:
- Detect slow Tor circuits
- Offer "low bandwidth mode": Disable animations, reduce ping frequency
- Compress messages (zstd)
- Prioritize text over future media

### User Notifications

**Desktop Notifications**:
- Optional system notifications for new messages
- Privacy-respecting: "New message from Bob" (no content preview)
- Configurable: Per-friend, channels only, all, or none
- Honors system Do Not Disturb settings

**In-App Notifications**:
- Friend requests in dedicated notification center
- Channel post notifications (if subscribed)
- System messages (Tor connection lost, update available)

**Sound Alerts**:
- Optional audio alerts for new messages
- Themeable (custom sound files)
- Volume control in settings
- Mute during specific hours

**Spam Protection**:
- Rate limiting on friend requests (max 5 per hour from same .onion)
- Option to block .onion addresses
- Report spam mechanism (future: shared blocklist)

---

## Section 8: Implementation Roadmap & Testing

### Development Phases

**Phase 1: Core Foundation** (Est. 4-6 weeks)
- Set up Rust project structure
- Integrate Tor (arti library)
- Hidden service creation and management
- Basic ratatui UI skeleton (layout, navigation)
- Friend management (add, approve, list)
- SQLCipher database setup and schema
- 1-on-1 messaging (persistent mode only)
- Basic end-to-end encryption (Double Ratchet)

**Deliverable**: MVP with functional 1-on-1 encrypted chat over Tor

**Phase 2: Enhanced Messaging** (Est. 3-4 weeks)
- Ephemeral mode (RAM-only messages)
- Timer-based deletion implementation
- Burn-after-reading implementation
- Online/offline status detection
- Message delivery receipts
- Read receipts
- Typing indicators

**Deliverable**: Feature-complete messaging with ephemeral modes

**Phase 3: Broadcast Channels** (Est. 2-3 weeks)
- Channel creation and subscription
- Public vs friends-only channel logic
- Channel post composition UI
- Channel feed UI
- Post delivery to subscribers
- Retention policies

**Deliverable**: Working broadcast channel system

**Phase 4: Polish & Theming** (Est. 3-4 weeks)
- Animations and visual effects
- Theming system implementation
- Default themes (dark, light, cyberpunk, minimal, Rosé Pine variants)
- Vanity .onion mining UI (beautiful progress indicators)
- Settings/preferences UI
- Keybindings customization
- Performance optimizations

**Deliverable**: Production-ready v1.0

**Phase 5: Hardening** (Est. 2-3 weeks)
- Security audit and fixes
- Comprehensive error handling
- Backup/restore system
- Migration tools
- Documentation
- Package for distributions (deb, rpm, Homebrew, AUR)

**Deliverable**: Stable v1.0 release

### Testing Strategy

**Unit Tests**:
- Cryptography: Key derivation, encryption, signing
- Friend code generation and validation
- Message protocol encoding/decoding
- Database operations
- Ephemeral message lifecycle

**Integration Tests**:
- Tor hidden service setup
- End-to-end message flow
- Database persistence and recovery
- Channel subscription and delivery
- Friend request approval flow

**End-to-End Tests**:
- Spin up two app instances locally
- Simulate complete user flows
- Automated UI testing (ratatui test harness)
- Performance benchmarks (latency, throughput)

**Security Testing**:
- Cryptography review by security experts
- Penetration testing (attempt to break encryption)
- Fuzzing message protocol
- Memory safety checks (MIRI, Valgrind)
- Dependency vulnerability scanning

**User Testing**:
- Alpha/beta testers from privacy community
- Feedback on UI/UX (especially theming)
- Real-world Tor latency testing
- Cross-platform testing (Linux distros, macOS versions, BSDs)

**Stress Testing**:
- Large friend lists (100+ friends)
- Large message history (10,000+ messages)
- Slow Tor circuits
- Rapid message sending
- Database size growth

### Key Dependencies

**Core Libraries**:
- `ratatui` (v0.27+) - TUI framework
- `arti` or `tor-client` - Tor integration
- `rusqlite` (v0.31+) - SQLite bindings
- `sqlcipher` - SQLite encryption
- `tokio` (v1.35+) - Async runtime
- `serde` (v1.0+) - Serialization

**Cryptography**:
- `ed25519-dalek` (v2.1+) - Signing
- `x25519-dalek` (v2.0+) - Key exchange
- `chacha20poly1305` (v0.10+) - AEAD encryption
- `argon2` (v0.5+) - Password KDF
- `double-ratchet` or custom Signal protocol implementation

**Utilities**:
- `clap` (v4.5+) - CLI argument parsing
- `tracing` (v0.1+) - Logging
- `thiserror` (v1.0+) - Error handling
- `anyhow` (v1.0+) - Error propagation
- `toml` (v0.8+) - Config file parsing

### Project Structure

```
torrent-chat/
├── Cargo.toml
├── Cargo.lock
├── README.md
├── LICENSE
├── docs/
│   ├── plans/                    # Design documents
│   ├── architecture.md           # Technical architecture
│   ├── security.md               # Security documentation
│   ├── user-guide.md             # User manual
│   └── api.md                    # Internal API docs
├── src/
│   ├── main.rs                   # Entry point
│   ├── app.rs                    # Main application state
│   ├── cli.rs                    # CLI argument parsing
│   ├── config/
│   │   ├── mod.rs
│   │   ├── settings.rs           # User settings
│   │   └── theme.rs              # Theme configuration
│   ├── tor/
│   │   ├── mod.rs
│   │   ├── hidden_service.rs     # Hidden service management
│   │   ├── circuit.rs            # Tor circuit handling
│   │   └── vanity.rs             # Vanity address mining
│   ├── crypto/
│   │   ├── mod.rs
│   │   ├── identity.rs           # Identity keys
│   │   ├── ratchet.rs            # Double Ratchet implementation
│   │   ├── kdf.rs                # Key derivation
│   │   └── signing.rs            # Message signing
│   ├── db/
│   │   ├── mod.rs
│   │   ├── schema.rs             # Database schema
│   │   ├── migrations.rs         # Schema migrations
│   │   ├── messages.rs           # Message storage
│   │   ├── friends.rs            # Friend list storage
│   │   └── channels.rs           # Channel storage
│   ├── protocol/
│   │   ├── mod.rs
│   │   ├── message.rs            # Message protocol
│   │   ├── friend_request.rs     # Friend request protocol
│   │   ├── friend_code.rs        # Friend code generation
│   │   └── channel.rs            # Channel protocol
│   ├── net/
│   │   ├── mod.rs
│   │   ├── connection.rs         # Connection management
│   │   ├── delivery.rs           # Message delivery queue
│   │   └── presence.rs           # Online status detection
│   ├── ephemeral/
│   │   ├── mod.rs
│   │   ├── timer.rs              # Timer-based deletion
│   │   └── burn.rs               # Burn-after-reading
│   ├── channels/
│   │   ├── mod.rs
│   │   ├── broadcast.rs          # Broadcast logic
│   │   └── subscription.rs       # Subscription management
│   ├── ui/
│   │   ├── mod.rs
│   │   ├── app_ui.rs             # Main UI coordinator
│   │   ├── layout.rs             # Layout management
│   │   ├── sidebar.rs            # Sidebar component
│   │   ├── conversation.rs       # Conversation view
│   │   ├── input.rs              # Message input
│   │   ├── channel_feed.rs       # Channel feed view
│   │   ├── settings.rs           # Settings UI
│   │   ├── friend_request.rs     # Friend request UI
│   │   ├── vanity_miner.rs       # Vanity mining UI
│   │   ├── animations.rs         # Animation helpers
│   │   └── theme.rs              # Theme rendering
│   └── error.rs                  # Error types
├── themes/
│   ├── dark.toml
│   ├── light.toml
│   ├── cyberpunk.toml
│   ├── minimal.toml
│   ├── rose-pine.toml
│   ├── rose-pine-moon.toml
│   └── rose-pine-dawn.toml
├── tests/
│   ├── integration/
│   │   ├── messaging.rs
│   │   ├── friend_management.rs
│   │   └── channels.rs
│   └── e2e/
│       └── two_instances.rs
└── benches/
    └── message_throughput.rs
```

### Configuration Paths

**Linux**:
- Config: `~/.config/torrent-chat/`
- Data: `~/.local/share/torrent-chat/`
- Cache: `~/.cache/torrent-chat/`

**macOS**:
- Config: `~/Library/Application Support/torrent-chat/`
- Data: `~/Library/Application Support/torrent-chat/`
- Cache: `~/Library/Caches/torrent-chat/`

**BSD**:
- Config: `~/.config/torrent-chat/`
- Data: `~/.local/share/torrent-chat/`
- Cache: `~/.cache/torrent-chat/`

### Performance Targets

- Cold start time: < 3 seconds (including Tor bootstrap)
- Message send latency: 300-800ms (Tor overhead)
- UI framerate: 60 FPS (smooth animations)
- Memory usage: < 100MB idle, < 500MB with large history
- Database size: ~1MB per 10,000 messages

---

## Conclusion

This design provides a comprehensive foundation for building **torrent-chat**, a privacy-first TUI chat application. The architecture balances strong security guarantees with usability, leveraging Tor for anonymity, modern cryptography for confidentiality, and ratatui for a beautiful user experience.

**Key Differentiators**:
- No phone numbers, no central servers, no registration
- True peer-to-peer over Tor
- Ephemeral messaging with multiple modes
- Broadcast channels for one-to-many communication
- Beautiful, themeable TUI that respects terminal culture

**Next Steps**:
1. Set up project repository and initial Rust structure
2. Prototype Tor hidden service integration
3. Build basic TUI layout and navigation
4. Implement core messaging protocol
5. Iterate based on user feedback

---

**Document Status**: Ready for implementation planning
**Last Updated**: 2026-02-06
