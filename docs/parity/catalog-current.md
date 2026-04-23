# Slack Web/Desktop Interface Catalog

- Generated for parity wave 1
- Scope source: `.omx/plans/prd-research-slack-client-webui-interfaces-for-libslack.md`
- Seed evidence: `.omx/research/slack-web-desktop/seed-observations-2026-04-21.md`
- In scope: stable Slack web + desktop behavior, including private first-party interfaces when observed and stable enough to classify
- Out of scope: mobile-only surfaces, admin/org-management tooling

## Status legend
- `implemented-observed` — implemented in `libslack` and backed by concrete observation evidence in this catalog
- `implemented-seeded` — implemented in `libslack`, but this wave only has repo/docs seed evidence; explicit first-party traffic observation is still pending
- `decode-covered` — parser/event decoding exists, but full product-level support is not yet claimed
- `pending` — recognized for the parity program but not implemented in this wave
- experiment-only entries, when present, are marked explicitly in Notes as `experiment-only` and should not be treated as stable parity by default
- `excluded` — intentionally outside scope for this phase
- `fragile` — private/undocumented and likely to churn; implementation should wait for stronger observation evidence

## Family: bootstrap / session
| Interface | Source | Classification | Status | Evidence | Notes |
| --- | --- | --- | --- | --- | --- |
| `auth.test` | public-doc | public-documented | implemented-seeded | seed note + existing code | current session bootstrap sanity check |
| `rtm.connect` | public-doc / legacy | public-documented | implemented-seeded | seed note + existing code | current realtime bootstrap, but Slack positions RTM as legacy |
| desktop credential extraction (`xoxc` + `d` cookie) | desktop app local state | private-stable | implemented-seeded | seed note + existing code | first-party-adjacent auth bootstrap already in use |
| `client boot / web bootstrap payloads` | first-party web/desktop | private-stable | pending | seed note says observation still needed | key future target for fuller parity |
| `apps.connections.open` / Socket Mode | public-doc | public-documented | pending | official docs only | relevant modernization path; not part of current user-session flow |

## Family: conversations / workspace metadata
| Interface | Source | Classification | Status | Evidence | Notes |
| --- | --- | --- | --- | --- | --- |
| `conversations.list` | public-doc | public-documented | implemented-seeded | seed note + existing code | current broad listing baseline |
| `conversations.info` | public-doc | public-documented | implemented-seeded | public-doc seeded + wave-1 tests | wrapper added in this wave; first-party traffic proof still pending |
| `conversations.members` | public-doc | public-documented | implemented-seeded | public-doc seeded + wave-1 tests | wrapper added in this wave; first-party traffic proof still pending |
| `conversations.open` | public-doc | public-documented | implemented-seeded | public-doc seeded + wave-1 tests | wrapper added in this wave; first-party traffic proof still pending |
| `users.conversations` | public-doc | public-documented | implemented-seeded | public-doc seeded + tranche tests | wrapper added in the current tranche; first-party traffic proof still pending |
| `users.channelSections.list` | first-party desktop/web | private-stable | implemented-seeded | seed note + existing code | private navigation/sidebar groundwork already present |

## Family: messages / threads / reactions / pins
| Interface | Source | Classification | Status | Evidence | Notes |
| --- | --- | --- | --- | --- | --- |
| `conversations.history` | public-doc | public-documented | implemented-seeded | seed note + existing code | current message history baseline |
| `conversations.replies` | public-doc | public-documented | implemented-seeded | seed note + existing code | current thread baseline |
| `conversations.mark` | public-doc | public-documented | implemented-seeded | seed note + existing code | current read-state baseline |
| `chat.postMessage` | public-doc | public-documented | implemented-seeded | seed note + existing code | current message-send baseline |
| `reactions.add` / `reactions.remove` | public-doc | public-documented | implemented-seeded | seed note + existing code | current reactions baseline |
| `pins.list` | public-doc | public-documented | implemented-seeded | public-doc seeded + wave-1 tests | wrapper added in this wave; first-party traffic proof still pending |
| richer message action surfaces | first-party web/desktop | private-stable | pending | seed note says later observation needed | bookmark/canvas/share/message action flows need later discovery |

## Family: users / presence / profiles
| Interface | Source | Classification | Status | Evidence | Notes |
| --- | --- | --- | --- | --- | --- |
| `users.list` / `users.info` | public-doc | public-documented | implemented-seeded | seed note + existing code | current user baseline |
| `users.profile.get` | public-doc | public-documented | implemented-seeded | public-doc seeded + wave-1 tests | wrapper added in this wave; first-party traffic proof still pending |
| `team.profile.get` | public-doc | public-documented | implemented-seeded | public-doc seeded + wave-1 tests | wrapper added in this wave; useful for profile-field taxonomy |
| websocket `presence_change` | public-doc / RTM family | public-documented | decode-covered | realtime unit test only | parser coverage exists; full presence product support is not yet claimed |
| richer presence / availability surfaces | first-party web/desktop | private-stable | pending | seed note says observation still needed | likely connected to client boot and live presence pipelines |

## Family: search
| Interface | Source | Classification | Status | Evidence | Notes |
| --- | --- | --- | --- | --- | --- |
| `search.messages` | public-doc | public-documented | implemented-seeded | seed note + existing code | current search baseline |
| `search.files` | public-doc | public-documented | implemented-seeded | public-doc seeded + tranche tests | wrapper added in the current tranche; first-party traffic proof still pending |

## Family: files / media metadata
| Interface | Source | Classification | Status | Evidence | Notes |
| --- | --- | --- | --- | --- | --- |
| `files.getUploadURLExternal` / `files.completeUploadExternal` | public-doc | public-documented | implemented-seeded | seed note + existing code | current upload flow baseline |
| raw private file download | first-party/web user session behavior | private-stable | implemented-seeded | seed note + existing code | current cookie-auth download path |
| `files.info` | public-doc | public-documented | implemented-seeded | public-doc seeded + wave-1 tests | wrapper added in this wave; first-party traffic proof still pending |
| `files.list` | public-doc | public-documented | implemented-seeded | public-doc seeded + wave-1 tests | wrapper added in this wave; first-party traffic proof still pending |
| huddles/calls/media signaling | first-party web/desktop | private-stable | pending | seed note lists this as follow-up evidence | still in scope, but deferred beyond wave 1 |

## Family: excluded for this phase
| Family / surface | Reason |
| --- | --- |
| mobile-only APIs and mobile-specific parity | explicitly excluded by spec |
| admin/org-management tooling | explicitly excluded by spec |

## Wave 1 completion accounting
Implemented-seeded in this wave:
- `conversations.info`
- `conversations.members`
- `conversations.open`
- `users.conversations`
- `users.profile.get`
- `team.profile.get`
- `search.files`
- `files.info`
- `files.list`
- `pins.list`

Decode-covered in this wave:
- websocket `presence_change`

Implemented-seeded in the current program state:
- desktop credential extraction
- `users.channelSections.list`
- raw private file download

Explicitly pending after the current tranche:
- first-party web bootstrap / client boot payloads
- richer message action / bookmark / canvas surfaces
- richer presence / availability surfaces
- huddles/calls/media signaling and metadata
- broader private-stable web/desktop interface families discovered in later observation passes
