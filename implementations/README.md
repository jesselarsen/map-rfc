# Reference implementations

Reference implementations of MAP live here, one subdirectory per language,
tracking the reference-implementation guidance and conformance program in
[`../spec/draft-map-ops-00.md`](../spec/draft-map-ops-00.md).

- `rust/` — primary reference implementation.
- `go/` — secondary reference implementation, used for the required
  independent-interoperating-implementation pair before any draft can leave
  draft status (`draft-map-ops-00.md` §6.1).

No document leaves draft status without two independent interoperating
implementations. Each implementation should track which spec revision
(`-00`, `-01`, ...) it targets and report interop test-suite results against
[`../test-vectors/map-test-vectors.md`](../test-vectors/map-test-vectors.md).

These directories are currently empty — populate as implementation work
starts.
