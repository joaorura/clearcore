# M1 Frozen Golden Fixture

Status: `BLOCKED_PENDING_GOLDEN`.

No frozen output is committed. `frozen-reference.json` may be created only on the designated isolated host after fresh M0 verification and approval of the local corpus described in `fixtures/corpus/README.md`. Normal tests and benchmark runs are read-only and must not derive expected output from the output under test.

Schema version 1 orders cases by stable case ID and frames by input frame index. Each frame contains exactly 480 finite `f32` samples. Provenance binds the generator source revision and command, UTC generation time, isolated/offline host identity, approved archive and M0 record digests, authorized key ID, backend descriptor, corpus checksums, output checksum and frame count, mono 48 kHz format, 1,440-sample algorithmic latency, exact absolute and relative numerical tolerances, and a separately versioned quality metric with normalization, threshold, and observed reference value. The canonical JSON digest of the provenance object is stored as `provenance_sha256`.

Any missing case, reordered frame, truncated data, non-finite sample, asset/corpus/descriptor drift, tolerance change, quality threshold change, or provenance digest mismatch is `BLOCKED_PENDING_GOLDEN`; it is never repaired by widening a runtime epsilon.
