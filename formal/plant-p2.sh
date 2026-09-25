#!/bin/bash
# Planted-bug validation for P2 (hello-cb.pv Q2, injective anti-relay).
# Each variant removes one mechanism Q2 depends on; Q2 MUST come out FALSE for
# every variant. A variant that leaves Q2 true means the query has no teeth
# for that mechanism. Works on scratch copies in /tmp; the repo is not modified.
# Run under WSL2:  bash formal/plant-p2.sh
set -u
HERE="$(cd "$(dirname "$0")" && pwd)"
SRC="$HERE/hello-cb.pv"
W="$(mktemp -d)"
PV="${PROVERIF:-$HOME/.opam/default/bin/proverif}"
fail=0

run() {  # name, sed-args...
  local name="$1"; shift
  sed "$@" "$SRC" > "$W/$name.pv"
  if ! grep -q PLANTED "$W/$name.pv"; then echo "$name: plant did not apply"; fail=1; return; fi
  local r
  r="$("$PV" "$W/$name.pv" 2>&1 | grep -E '^RESULT inj-event')"
  if [[ "$r" == *" is false." ]]; then echo "$name: caught (Q2 false)"; else echo "$name: NOT CAUGHT: $r"; fail=1; fi
}

run no-envelope-signature \
  -e 's/    let chk2 = checksign(sealedEnv, claimedPk) in/    (* PLANTED *)/' \
  -e 's/    if chk2 = sealedBody then/    (* PLANTED *)/'
run no-cb-check -e 's/    if cbIn = cb then/    (* PLANTED *)/'
run no-responder-nonce -e 's/    new nB: bitstring;.*/    let nB = msgidH in (* PLANTED *)/'

rm -rf "$W"
exit $fail
