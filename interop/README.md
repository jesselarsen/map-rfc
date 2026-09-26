# interop/ — live Rust↔Go interop matrix

Real MAP traffic between `map-rs` and `map-go`. See [`MATRIX.md`](MATRIX.md)
for the recorded result, the cell tables and the harness validation.

## Run it

```
pwsh run-matrix.ps1
```

The driver builds every peer and runs three parts:
- (a) each implementation's frozen-vector conformance;
- (b) the A2A binding over HTTP: 8 mixed-implementation failover cells,
  2 plain-client cells, and 2 a2a-sdk cells when `MAP_A2A_SDK_PY` is set;
- (c) the 14 core session cells over TCP.

It exits non-zero on any failure. It requires Go on PATH and the GNU Rust
toolchain with mingw on PATH (`memory.md`).

## The peers

- **A2A, Rust:** `map-a2a-provider` / `map-a2a-requester`
  (`cargo build -p map-a2a --features http`).
- **A2A, Go:** `map-go/cmd/map-a2a-provider` / `map-a2a-requester`, same
  flags and the same `MAP-DEMO-RESULT {json}` output.
- **Core, Rust:** `map-cli interop --role {dial|listen} --addr H:P --seed
  HEX32 --peer-seed HEX32 --scenario NAME`.
- **Core, Go:** `map-go/cmd/map-interop` with the same flags (no `interop`
  subcommand prefix).
- **SDK:** [`a2a-sdk/check_sdk.py`](a2a-sdk/check_sdk.py) with the
  unmodified A2A Python SDK (`a2a-sdk/requirements.txt`).

All of these are conformance harnesses, not products. The core peers drive
the `session` layer directly and use `send_raw` plus the channel-binding
accessor to craft adversarial frames. The A2A providers serve a
deterministic word generator and can simulate a crash with
`--fail-after-parts`.
