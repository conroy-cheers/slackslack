# Seed observations for Slack web/desktop parity

_Date: 2026-04-21_

This committed note mirrors the current non-secret evidence available before full
first-party traffic capture and normalization tooling are in place.

## Repo-backed observations
- `libslack` implements a core Slack Web API baseline for conversations, threads,
  messaging, reactions, users, search, file upload, and realtime bootstrap.
- Desktop credential extraction uses first-party local state (`xoxc-*` token from
  Slack's LevelDB and the `d` cookie from Slack's Chromium cookie store).
- A private first-party-adjacent method already exists: `users.channelSections.list`.
- Raw private file download is already exercised through authenticated first-party
  user-session semantics.

## Public-doc cross-checks completed
- `conversations.list`, `conversations.info`, `conversations.members`,
  `conversations.open`, `conversations.history`, `conversations.replies`,
  `conversations.mark`
- `chat.postMessage`
- `users.list`, `users.info`, `users.conversations`, `users.profile.get`, `team.profile.get`
- `search.messages`, `search.files`
- `emoji.list`
- `reactions.add`, `reactions.remove`
- `pins.list`
- `files.info`, `files.list`
- `files.getUploadURLExternal`, `files.completeUploadExternal`
- `rtm.connect`
- Web API rate-limit guidance and file-upload retirement notes

## Evidence limitations
- No normalized first-party web/devtools traffic fixtures are committed yet.
- No committed websocket capture set exists yet for non-RTM first-party flows.
- Huddles/calls/media remain cataloged but unobserved in committed artifacts.
