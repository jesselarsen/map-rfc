# interop/ — live Rust↔Go interop matrix (TASK-020)

The ops §6.1 gate: two independent MAP implementations completing real
sessions against each other over TCP. See [`MATRIX.md`](MATRIX.md) for the
recorded result and the scenario table.

## Run it

```
pwsh run-matrix.ps1
```

The driver builds both peer binaries, runs all 8 scenarios in both
orientations (16 cells), prints the result table, and exits non-zero on any
failure. Requires Go on PATH and the GNU Rust toolchain with mingw on PATH
(`memory.md` §5/§7).

## The peers

- **Rust:** `map-cli interop --role {dial|listen} --addr H:P --seed HEX32
  --peer-seed HEX32 [--issuer-seed HEX32] --scenario NAME`
- **Go:** `map-go/cmd/map-interop` with the same flags (no `interop`
  subcommand prefix).

A listener binds an ephemeral port (`--addr 127.0.0.1:0`) and prints
`LISTENING <addr>`; the driver reads the port and hands it to the dialer.
Every run ends with `OK <scenario>` or `FAIL <scenario>: <detail>`.

Both binaries are conformance harnesses, not products: they drive the
`session` layer directly and use `send_raw` + the channel-binding accessor
to craft adversarial frames. Nothing here ships.
