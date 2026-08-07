# Selvedge Core Roadmap

This document tracks the feature completion of the Core actor for the Selvedge Matrix client. This is intentionally broader than necessary, and some features will later be discarded if deemed to be out of scope.

---

### Milestone 1: Core Chat Primitives (Messaging & Timeline)

- [x] **Rich Text Formatting:** Compile Markdown to HTML locally and send `text_html` alongside `text_plain`.
- [x] **Emotes (`m.emote`):** Intercept `/me` commands in the Core to send the `m.emote` message type instead of standard text.
- [x] **Unknown Message Fallbacks:** Render the standard `body` string for unrecognized custom `msgtype` or event types.
- [x] **Notice Handling (`m.notice`):** Map `m.notice` events explicitly so the UI can render bot/bridge messages appropriately.
- [ ] **State Event Formatting:** Map timeline state events (joins, parts, bans) into structured, UI-ready formats like `StateEvent::MemberJoin("Alice")`.
- [x] **Virtual Timeline Items:** Map `VirtualTimelineItem::DayDivider` events from the SDK to the UI to display date separators.
- [x] **Local Echo Send States:** Map `EventSendState` (Sending, Sent, Failed) to the UI to display loading spinners or error indicators.
- [x] **Failed Message Retry:** Add a handler to re-attempt sending a local echo that was marked as `Failed`.
- [x] **Failed Message Cancel:** Add a handler to discard a failed local echo, removing it from the timeline.

- [x] **Max Media Upload Check:** Fetch the homeserver's `m.upload.size` limit before initiating `send_media` to reject oversized files immediately.
- [x] **Media Metadata & Thumbnails:** Generate and attach thumbnails, blurhashes, width, and height to media uploads to prevent UI layout shifts.
- [x] **Media Captions:** Update `send_media` to accept and attach optional text captions to image, video, and file uploads.
- [x] **On-Demand Media Fetching:** Expose only thumbnails initially, deferring full-res media streams until explicitly requested by the UI.

- [x] **Reply HTML Fallbacks:** Append the standard HTML blockquote fallback to replied messages for backwards compatibility.
- [x] **Nested Reply Stripping:** Strip existing `<mx-reply>` blocks when replying to an existing reply to prevent infinite quote nesting.
- [x] **Member Autocomplete Provider:** Add a Core helper to quickly filter and return room members matching a string prefix to fuel UI mention menus.
- [x] **Intentional Mentions (MSC3952):** Populate the `m.mentions` array in the event content when tagging users.
- [x] **Mention Pill Resolution:** Provide a Core helper to synchronously resolve Matrix IDs in HTML into `MemberProfile` data for pretty UI rendering.
- [x] **Code Block Metadata:** Preserve `language-*` classes on `<pre><code>` blocks in the Markdown parser to allow syntax highlighting.
- [x] **Spoiler Formatting:** Support parsing and sending spoilers using the Matrix `<span data-mx-spoiler>` standard.
- [x] **Mathematical Formatting (MSC3193):** Render LaTeX/Math equations using the `data-mx-maths` HTML attribute within the Markdown compiler.

- [x] **Message Editing:** Implement sending messages with the `m.replace` relation to edit previously sent messages.
- [x] **Message Redaction (Deletion):** Implement sending `m.room.redaction` events so users can delete their own messages.
- [x] **Pending Edit & Redaction States:** Expose the pending `EventSendState` of offline edits and redactions so the UI can display loading indicators.
- [x] **Emoji Reactions:** Add endpoints to send and remove `m.reaction` events.

- [x] **Send Read Receipts:** Create a handler to send `m.receipt` events to the server as the user reads new messages.
- [x] **Read Receipt Debouncing:** Implement a debounce queue in the Core to prevent rate-limiting when the user rapidly scrolls. (delegated to the UI)
- [x] **Private Read Receipts (MSC2285):** Support sending `m.read.private` instead of `m.read` based on a user preference toggle to hide read status from others.
- [x] **Extract Read Receipts:** Map incoming `read_receipts` from the SDK timeline items so the UI can render "read by" avatars.
- [x] **Fully Read Markers:** Create a handler to send the `m.fully_read` account data event to sync the user's scroll position across devices.
- [x] **The "New Messages" Divider:** Map the `VirtualTimelineItem::ReadMarker` event to allow the UI to render a separator line for unread messages.

- [x] **Timeline Pagination Boundaries:** Expose flags for `start_of_room` and `live_edge` to control UI "Load More" spinners.
- [ ] **Event Permalink Generation:** Add a Core helper to construct `matrix.to` or `matrix://` URIs for sharing specific messages.
- [ ] **Jump to Message:** Add a handler to dynamically load a focused timeline around a specific event ID when a user clicks a reply block.
- [ ] **Composer Drafts:** Persist in-progress text input per room to IndexedDB so unsent messages survive page refreshes.
- [x] **Offline Queuing:** Implement a local queue to hold messages sent while disconnected, automatically flushing when reconnected.
- [ ] **Push Rule Evaluation:** Wire up the SDK's push rule evaluator to dynamically flip an `is_highlight` boolean on incoming messages.
- [ ] **Decryption Error States (UTD):** Map specific `EncryptionStatus` error types to display actionable UI placeholders.
- [ ] **Per-Message Trust Shields:** Expose a warning flag on timeline items if a previously verified user sends a message from an unverified session.

---

### Milestone 2: Communities & Discovery

- [ ] **Direct Messages (DMs):** Manage the `m.direct` account data event to classify 1:1 chats.
- [ ] **User Directory Search:** Search for users by Matrix ID/Name to start new DMs.
- [ ] **Room Invites:** Add an endpoint to invite other users to a room.
- [ ] **Room Settings:** Add handlers to modify existing room state events (name, topic, avatar).
- [ ] **Room Alias Management:** Allow admins to create, remove, and set the Canonical Alias (`m.room.canonical_alias` and `m.room.aliases`).
- [ ] **Room Visibility & Access Control:** Manage `m.room.join_rules` (including Space-restricted joins), `m.room.history_visibility`, `m.room.guest_access`, and directory publishing.
- [ ] **Power Levels:** Fetch, view, and modify `m.room.power_levels` (admin/moderator permissions).
- [ ] **Granular Power Levels & Muting:** Configure specific action permissions (e.g., who can `@room`) and mute users by setting their power level to negative.
- [ ] **Basic Moderation:** Add handlers to kick, ban, and unban users.
- [ ] **Public Room Search:** Implement directory searching to find/join public aliases.
- [ ] **Matrix Spaces:** Expose space management (MSC1772) from the `RoomListService`.
- [ ] **Space Hierarchy & Ordering:** Support adding/removing subspaces and implement the `order` property within `m.space.child` events for channel sorting.
- [ ] **Room Server ACLs:** Support viewing and modifying `m.room.server_acl` events to block malicious homeservers.

---

### Milestone 3: Identity & Trust

- [ ] **Account Registration:** Implement the signup flow for new Matrix accounts.
- [ ] **SSO / OIDC Login:** Support modern identity providers beyond standard username/password.
- [ ] **Server Discovery:** Resolve `.well-known/matrix/client` endpoints automatically from a user's Matrix ID.
- [ ] **3PID / Identity Server Integration:** Discover and invite users via linked email addresses and phone numbers.
- [ ] **Profile Management:** Update global display name and avatar (`m.room.member` state).
- [ ] **Presence & Status:** Sync and display `m.presence` (Online/Offline) and allow users to set custom status messages.
- [ ] **User Blocklist:** Implement an "Ignore User" feature to hide messages from specific users.
- [ ] **Content Reporting:** Wire up `client.report_content(...)` to flag abusive messages to admins.
- [ ] **Account Deactivation:** Add a handler for the `POST /_matrix/client/v3/account/deactivate` endpoint to ensure GDPR compliance.
- [ ] **Change Password:** Support the UIAA flow to allow users to update their account password from within the app.
- [ ] **Secure Backup Management:** Enable users to generate new recovery keys or change their secure backup passphrase.
- [ ] **Login via QR Code (MSC3906):** Implement a flow allowing the web client to display a QR code for cross-device authentication.
- [ ] **Cross-Signing Identity Reset:** Add a handler to deliberately reset cross-signing keys for users permanently locked out of their old encrypted devices.

---

### Milestone 4: Quality of Life

- [ ] **Key Request Approvals:** Add a handler to verify and forward E2EE room keys to requesting devices.
- [ ] **Device Renaming:** Allow users to rename their sessions (e.g., "My Laptop").
- [ ] **Receiving Typing Sync:** Listen to ephemeral events and dynamically update the `typing_users` list.
- [ ] **Threading (MSC3440):** Support branching conversations via thread-specific timelines.
- [ ] **Push Rules:** Manage user notification preferences (e.g., "Mentions Only" or "Muted").
- [ ] **Web Push Notifications:** Register a Pusher to receive background notifications via the browser.
- [ ] **Mark as Unread:** Allow users to artificially mark rooms as unread.
- [ ] **Read Receipt Privacy ("Incognito"):** Add a toggle to disable sending `m.receipt` and `m.typing` events entirely.
- [ ] **Media Upload Progress:** Expose upload progress events via `ToShell` for loading bars.
- [ ] **Pinned Messages:** Handle pinning, unpinning, and fetching `m.room.pinned_events`.
- [ ] **Room Knocking (MSC2403):** Support requesting access to private rooms and approving knocks.
- [ ] **Widgets Integration:** Support embedding web apps (Jitsi, Etherpad) via `m.widget`.
- [ ] **Slash Commands:** Intercept commands like `/me`, `/join`, or `/shrug` to trigger actions locally.
- [ ] **Remote Session Management:** Build UI flows to view active sessions and remotely log out old devices to protect the account.
- [ ] **Data Saver Mode (Auto-Download Toggles):** Implement a setting to prevent the client from automatically fetching and decrypting media/thumbnails on metered connections.
- [ ] **Thread Inbox / Summary:** Fetch the cross-room or per-room "Threads Inbox" to display a unified list of active conversations.

---

### Milestone 5: Rich Media & Interactions

- [ ] **Voice Messages (MSC3245):** Add support for recording and rendering audio snippets.
- [ ] **WebRTC Calls (VoIP):** Implement 1:1 audio and video signaling (`m.call.invite`).
- [ ] **Polls (MSC3381):** Create polls, handle user votes (`m.poll.response`), and end them (`m.poll.end`).
- [ ] **Location Sharing (MSC3488):** Send and render static and live-updating map locations (`m.location`).
- [ ] **Stickers:** Implement the `m.sticker` event type and sticker pack integration.
- [ ] **Custom Emojis (MSC2545):** Support rendering custom community emojis in the timeline and sending them via the composer.
- [ ] **Emoji Autocomplete Provider:** Add a Core helper to rapidly filter and return available standard and custom emojis matching a string prefix.

---

### Milestone 6: Personalization & UX

- [ ] **Room Tagging & Sorting:** Implement support for the `m.tag` account data event (Favorites, Low Priority).
- [ ] **Accurate Unread/Mention Badges:** Extract and propagate exact unread/mention counts to the Shell.
- [ ] **URL Previews:** Call `/media/v3/preview_url` to fetch OpenGraph metadata and thumbnails for links.
- [ ] **Message Forwarding:** Create a flow to take an existing `EventItem`'s content and dispatch it to another `OwnedRoomId`.
- [ ] **Timeline Filtering:** Add a toggle to hide state events (joins/parts) from the timeline view.
- [ ] **Custom Account Data:** Use `m.account_data` to store and sync cross-device UI preferences (theme, layout).
- [ ] **Multi-Account Support:** Enable fast account switching by maintaining parallel states in the Core.

---

### Milestone 7: Edge Cases, Power Tools & Maintenance

- [ ] **Room Upgrades & Tombstones:** Handle `m.room.tombstone` events to migrate users gracefully to upgraded rooms.
- [ ] **Soft Logout & Token Refresh:** Pause sync and prompt re-authentication without wiping local E2EE keys.
- [ ] **Room Media Gallery / Shared Files:** Filter a room's history to build a "Shared Media" sidebar.
- [ ] **Global Cross-Room Search:** Perform searches across all of the user's encrypted and unencrypted rooms simultaneously.
- [ ] **Local Encrypted Search Indexing:** Build and maintain a local full-text search index to enable querying decrypted messages.
- [ ] **Export Chat History:** Paginate and write a room's history to a downloadable file.
- [ ] **Device Verification Prompts:** Show a toast UI when an incoming verification request from an alt-device is detected.
- [ ] **Dehydrated Devices (MSC3814):** Support offline E2EE device retrieval.
- [ ] **Server Notices:** Flag rooms with the `m.server_notice` tag so the Shell can render them as un-leaveable system alerts.

---

### Milestone 8: Enterprise & Community Admin Tools

- [ ] **Shared Moderation Policy Lists (MSC2313):** Subscribe to `m.policy.rule.user/server` lists to automatically ban known spammers.
- [ ] **Message Retention Policies (MSC1763):** Support `m.room.retention` for automatic message purging on a timer.
