# Phase 12.5: Remote service access and authorization

## Status

Draft contract for review. Phase 12.4 remains complete locally; no network
service or remote CLI is implemented by this design checkpoint. Freeze the
service boundary before adding network code.

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

Use operator-provisioned, high-entropy opaque bearer capabilities. Each grant
is bound to exactly one `RepositoryId` and one scope:

- `read`: discover refs and fetch canonical objects.
- `publish`: read access plus immutable object upload and named-reference CAS.

The service stores only a cryptographic digest of each token, compares token
verifiers in constant time, and never accepts credentials in URL query
parameters or request bodies. The HTTP semantic API uses the standard
`Authorization: Bearer` header. A wrong-repository request is rejected before
calling the Hub and returns the same `404` used for an unknown repository.

There is no token issuance, repository provisioning, binding mutation, or
retention-policy endpoint. Operators provision and revoke grants out of band;
revocation takes effect when the service reloads the grant file or restarts.
The local client receives a token at runtime from its caller. It must not write
the token into `.kat` repository metadata, a remote URL, logs, or diagnostics.
Later CLI work can connect that runtime input to an operating-system
credential store.

### 3. Physical Git endpoint

Expose the controlled backend only through Git smart HTTP over HTTPS. The
service-side gateway uses the same grant registry and token as the semantic
API: Git HTTP Basic authentication carries the opaque token as the password;
the username is a fixed non-identity label. The gateway authorizes
`git-upload-pack` with `read` and `git-receive-pack` with `publish`, and maps
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

1. Read capability can list refs and fetch exact object bytes, but cannot
   upload objects or CAS a ref; publish capability can perform both.
2. Missing credentials, invalid credentials, cross-repository paths, and
   wrong-scope requests fail without revealing whether another repository or
   object exists.
3. No request can create, replace, or delete the Hub-to-Git binding or change
   the retention policy.
4. A valid semantic publication and fetch use the fixed paired Git backend;
   a missing physical snapshot or mismatched backend prevents CAS.
5. Git smart HTTP fetch and push use the same repository grants: read-only
   tokens cannot push, publish tokens can fetch and push only their bound
   repository, and direct unauthenticated Git access is unavailable.
6. An interrupted physical or semantic upload leaves named-reference authority
   unchanged; retry succeeds. A stale CAS leaves the current target unchanged.
7. The HTTP client rejects non-HTTPS remote URLs except explicit loopback test
   setup, and no credential is serialized into repository configuration or
   emitted in logs.
8. Fetch through the network client preserves the receiving local ref, accepted
   state, workspace base, active lineage, and working tree.

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
