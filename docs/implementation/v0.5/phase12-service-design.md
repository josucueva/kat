# Phase 12.5: Remote service access and authorization

## Status

Frozen contract. Phase 12.4 remains complete locally; no network service or
remote CLI is implemented by this design checkpoint. Implement the shared
authorization verifier and HTTP boundary tests before exposing endpoints.

## Goal

Expose the trusted Hub and its paired physical Git backend to remote clients
without changing the frozen `SemanticTransport` or `WorkspaceTransport`
traits, and without allowing ordinary clients to change Hub repository
bindings or retention policy.

## Boundary

```text
RemoteCoordinator
    |
    +-- SemanticTransport
    |       |
    |       v
    |   HTTPS JSON API + repository-scoped authorization
    |       |
    |       v
    |   TrustedHubSemanticTransport
    |
    +-- WorkspaceTransport
            |
            v
        Git smart HTTP + the same authorization grants
            |
            v
        KAT-controlled Git backend (private to the service)
```

The Hub remains the authority for KAT named references. The controlled Git
backend stores physical snapshot representations only. Both endpoints resolve
the same repository-scoped grants; the Hub's immutable `RepositoryId` to
`WorkspaceBackendId` binding selects the only physical Git repository that a
request can reach.

## Decisions

### 1. Semantic protocol

Use a versioned HTTPS API at `/v1/repositories/{repository_id}`. The wire
adapter implements the existing `SemanticTransport` contract:

| Operation | Request | Successful response |
| --- | --- | --- |
| Discover refs | `GET .../references` | JSON named refs and complete unnamed tips |
| Fetch object | `GET .../objects/{object_id}` | Exact canonical bytes as `application/octet-stream`; `404` means absent |
| Store object | `PUT .../objects/{object_id}` | `204`; body is the exact canonical bytes |
| CAS named ref | `PUT .../reference-cas` | JSON `{ "name", "expected", "new" }`; `204` on success |

The CAS name is in the JSON body so names containing `/` do not depend on URL
path escaping. The server validates canonical object hashes and encodings,
reference names, semantic closure, the fixed backend binding, and the physical
snapshot closure before CAS, using the existing Hub core. Object writes remain
immutable and idempotent. The API has no operation to set a reference without
an expected value.

Use stable status mapping: `401` for a missing or invalid credential, `403` for
a valid credential without the required permission, `404` for an absent object
or repository outside the credential's scope, `409` for stale CAS, and `422`
for invalid object, ref, or closure data. Error responses use a small versioned
JSON envelope and do not expose filesystem paths, token values, or internal
storage details. Internal service failures return a generic `5xx` response.

### 2. Authorization model

Use operator-provisioned, opaque bearer capabilities with at least 256 bits of
randomness. The token is an opaque secret, not a self-contained JWT. The
operator-managed grant record binds its SHA-256 verifier to exactly one
`RepositoryId`, a scope, a required audience set, and a mandatory UTC expiry:

- `read`: discover refs and fetch canonical objects.
- `publish`: read access plus immutable object upload and named-reference CAS.
- `semantic-api` and `workspace-git` are distinct audiences. A credential
  usable by both endpoints must explicitly list both audiences.

One shared authorization verifier is called by both the Hub API and Git
gateway with `(token, repository_id, audience, action)`. It checks the token
digest in constant time, repository binding, audience, scope, and expiry for
every HTTP request. `publish` implies `read`. The semantic API uses the
standard `Authorization: Bearer` header. Git smart HTTP uses standard Basic
authentication over TLS: username `kat`, with the same opaque token as the
password. The gateway validates that password through the same verifier; the
username carries no identity or permission. Credentials are never accepted in
URL query parameters or request bodies.

Missing, malformed, unknown, expired, or revoked credentials return `401` and
never fall back to anonymous access. A valid token with insufficient scope
returns `403`; a request for a repository outside that token's binding returns
the same `404` as an unknown repository. The service does not reveal whether
the other repository or its objects exist.

There is no token issuance, repository provisioning, binding mutation, or
retention-policy endpoint. Operators provision and revoke grants out of band.
Every request checks the current shared policy snapshot; removing a grant and
atomically reloading the policy revokes it. A failed policy reload disables
authorization until a valid policy is loaded, rather than leaving stale grants
active. Expired grants are denied on the next request without reload.

The local client receives a token at runtime from its caller. It must not write
the token into `.kat` repository metadata, a remote URL, logs, or diagnostics.
Later CLI work can connect that runtime input to an operating-system
credential store.

### 3. Physical Git endpoint

Expose the controlled backend only through Git smart HTTP over HTTPS. The
gateway checks authorization at both ref discovery and pack execution:

| Git HTTP request | Required permission |
| --- | --- |
| `GET info/refs?service=git-upload-pack` | `read` |
| `POST git-upload-pack` | `read` |
| `GET info/refs?service=git-receive-pack` | `publish` |
| `POST git-receive-pack` | `publish` |

Reject unknown services, malformed paths, and mismatched discovery/pack
operations before invoking Git. Each request is independently authenticated;
discovery success does not authorize a later pack request. The gateway maps
the authenticated repository directly to its immutable configured backend.
`GitWorkspaceTransport` obtains the token at runtime through Git's credential
callback; it does not persist it in `workspace-remote.json`.

Do not expose the current unauthenticated `git://` loopback server as a remote
service. It remains a test fixture. The bare backend is private to the service
or gateway, which must preserve the create-only lineage-ref hook and must not
permit clients to access another repository's backend. TLS is required for
both the semantic API and Git smart HTTP. The client validates the server
certificate and does not fall back to plaintext. A reverse proxy may terminate
TLS, but the Hub and Git gateway must be reachable only on its trusted private
side.

### 4. Frozen semantics and failure behavior

- The network client implements `SemanticTransport`; Phase 11 signatures and
  local authority semantics do not change.
- Fetch remains observation-only. It may update remote-tracking refs after the
  coordinator verifies all content; it cannot update accepted state, local
  named refs, workspace base, active lineage, or the working tree.
- Publish transfers physical content and immutable semantic objects first.
  Hub-side closure verification and per-name CAS remain the visibility point.
- A failed request may leave immutable content that is not referenced. Retry
  is safe. A stale CAS returns conflict and never overwrites another target.
- Neither endpoint accepts a client-supplied backend path, `WorkspaceBackendId`,
  retention policy, or Git ref destination.
- Request-size and rate limits are enforced at the service boundary. A rejected
  oversized object must not create an object or advance a reference; limits may
  not change canonical identity or silently truncate data.

## Acceptance tests before service exposure

| ID | Required behavior |
| --- | --- |
| AUTH-01 | Missing credential is rejected. |
| AUTH-02 | Expired or revoked credential is rejected. |
| AUTH-03 | A grant for another repository is rejected without disclosing repository/object existence. |
| AUTH-04 | Read grant permits Hub discovery and exact object downloads. |
| AUTH-05 | Read grant rejects object upload and named-reference CAS. |
| AUTH-06 | Read grant permits Git fetch but rejects Git push. |
| AUTH-07 | Publish grant permits authorized object uploads and CAS. |
| AUTH-08 | Publish grant permits Git fetch and push only for its bound repository. |
| AUTH-09 | Ordinary clients cannot alter backend binding or retention policy. |
| AUTH-10 | Credentials are absent from logs, URLs, persisted state, and error output. |
| AUTH-11 | Authorization does not bypass semantic closure, physical identity, binding, retention, or CAS validation. |
| AUTH-12 | Hub and Git decisions use the same repository grant, scope, expiry, and revocation state. |
| AUTH-13 | Both `info/refs` discovery and pack `POST` are independently authorized; upload-pack requires read and receive-pack requires publish. |
| AUTH-14 | Direct unauthenticated Git access and anonymous fallback are unavailable. |
| AUTH-15 | Tests send real HTTP requests through the service boundary; they do not test only the verifier in isolation. |
| AUTH-16 | The HTTP client rejects non-HTTPS URLs except explicit loopback test setup and does not persist credentials. |
| AUTH-17 | Network fetch preserves the receiving local ref, accepted state, workspace base, active lineage, and working tree. |
| AUTH-18 | Interrupted upload leaves named-reference authority unchanged; retry succeeds, and stale CAS preserves the current target. |

## Trust assumptions and limits

Bearer capabilities are replayable if stolen, so TLS and secret-safe handling
are mandatory. The service process, operator grant file, TLS terminator, and
controlled Git host remain trusted. This design does not establish production
availability, backup/restore behavior, storage durability, or protection from
unrestricted host administrators; those remain deployment acceptance work.

## References

- [RFC 6750: Bearer Token Usage](https://www.rfc-editor.org/rfc/rfc6750.html)
  requires TLS for bearer-token use.
- [Git HTTP protocol](https://git-scm.com/docs/http-protocol) describes smart
  HTTP and standard HTTP authentication; [git-http-backend](https://git-scm.com/docs/git-http-backend)
  provides Git's server-side HTTP transport.
