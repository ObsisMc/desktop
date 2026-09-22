# Controller–Cloud contract

English | [中文](controller-cloud-contract.zh.md)

The API Server (Ora Cloud, Go) calls the Controller over gRPC. The contract is the `.proto` files
under [`proto/`](../../proto/ora/controller/v1/controller.proto) in this repository, owned by the
Controller as the server. Both sides generate code from the same files; nothing is aligned by hand.
Tenancy stays in Cloud: the contract carries only identities the API Server has already authorized
and has no tenant, user or membership fields. The transport and serving side are described by the
[Controller runtime](../controller/local-runtime.md); this page covers the contract itself and how
it changes.

## Package and surface

`ora.controller.v1` defines `ControllerService` with `AcceptClone`, `ListOperations` and
`GetOperation`. Semantics mirror the transitional JSON clone API: acceptance returns only after the
intent is durably committed, repeating a request identity with the same input returns the original
receipt, an operation without a terminal state is `pending` rather than failed, and an unknown
execution is `NOT_FOUND` rather than an empty result.

Failures use the canonical gRPC status code as the primary classification and attach
`ora.controller.v1.ErrorDetail` as a `google.rpc.Status` detail, so a caller may branch on either
without parsing messages:

| `ErrorCode` | gRPC status | Meaning |
|---|---|---|
| `CONFLICT` | `ABORTED` | Identity or input conflicts with a durable record; retrying the same call cannot succeed |
| `INVALID_INPUT` | `INVALID_ARGUMENT` | The request itself is malformed |
| `UNAVAILABLE` | `UNAVAILABLE` | Persistence is temporarily unavailable; nothing was accepted and the call may be retried |
| `NOT_FOUND` | `NOT_FOUND` | No accepted operation with that execution identity |

## Generated code

`crates/controller-proto` (`ora-controller-proto`) holds the Rust output under `src/gen/` and
nothing else; it is committed. `buf` generates it with pinned remote plugins
(`neoeinstein-prost`, `neoeinstein-tonic`), so regeneration needs network but no local `protoc`.

- `task proto:lint`: `buf lint` with the `STANDARD` rules.
- `task proto:generate`: regenerate `crates/controller-proto/src/gen`.
- `task proto:check`: regenerate and fail on any diff; part of `task lint:crates`.
- `task proto:breaking`: `buf breaking` against `PROTO_BASE_REF` (default `main`). Breaking changes
  are refused on `v1`; a new major surface is a new `v2` package beside it.

Cloud consumes the same files from a git tag of this repository (`controller-proto/vX.Y.Z`) with
its own `buf` configuration and commits the generated Go package; it never copies the `.proto`.

## Changing the contract

1. Edit `proto/ora/controller/v1/*.proto`; run `task proto:lint` and `task proto:breaking`.
2. Run `task proto:generate` and commit the regenerated crate with the change.
3. After merge, tag the commit `controller-proto/vX.Y.Z`.
4. In Cloud, update the tag in its `buf.gen.yaml`, regenerate, and build. A compile failure is the
   only alignment signal needed; there is no field-by-field review.

Structural alignment is what generation proves. Behavioral alignment is Cloud's integration test
against a real `ora-controller`, registered once the contract is stable.
