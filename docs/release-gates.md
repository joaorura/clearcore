# Release Gates

## M0 model asset gate

The only permitted release asset identifier is `df-compatible-release-asset-v1`. M0 remains blocked unless all of the following are true:

- Candidate provenance has the exact source `urn:sha256:<asset-sha256>`, exact asset identifier, SHA-256, code license, weight license, and conversion terms.
- Legal review has reviewer identity, ISO `YYYY-MM-DD` date, redistribution terms, conversion terms, legal approval ID, and explicit code and weight license review matching candidate provenance.
- Approval has the same asset identifier, candidate asset SHA-256, legal approval ID, redistribution terms, and conversion terms as the records it approves. It also contains `candidate_record_sha256` and `legal_review_record_sha256`, computed from the complete validated candidate and legal-review JSON documents using canonical UTF-8 JSON.
- Approval has an approver identity, ISO `YYYY-MM-DD` date, `approved_for_release: true`, a public-key ID, and a valid Ed25519 signature.
- The local blob is named `df-compatible-release-asset-v1.bin` and its SHA-256 matches both candidate and approval records.
- The public-key ID is `sha256:` followed by the lowercase SHA-256 of the public key's DER SubjectPublicKeyInfo encoding, and it must appear in the repository-controlled `governance/model-assets/trust-policy.json`. The production CLI resolves this path itself; callers cannot select another policy.

Approval signatures use base64-encoded raw Ed25519 signatures. The signed payload is UTF-8 JSON made from the complete approval object after removing only `signature`, with keys sorted, no insignificant whitespace, Unicode emitted directly, and non-finite numbers rejected. The signed approval therefore binds the canonical SHA-256 digests of both complete reviewed records. In Python terms, canonical records use `json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False, allow_nan=False).encode("utf-8")` before SHA-256.

Every missing, empty, malformed, inconsistent, hash-mismatched, key-mismatched, or signature-invalid input exits with code 2 and atomically writes `BLOCKED_NO_APPROVED_ASSET`. The verifier determines a unique `--status-out` before strict parsing, otherwise uses `docs/evidence/m0-asset-gate.json`, and replaces stale evidence before reading approval inputs. All verification inputs must be regular, non-symlink files read from stable snapshots. Approved evidence records the asset, candidate-record, and legal-review-record SHA-256 values, the authorized key ID, and a UTC verification timestamp.

## Production evidence command

Run from the repository root:

```sh
python3 scripts/verify-m0-asset.py --candidate governance/model-assets/df-compatible-release-asset-v1/candidate-provenance.json --legal-review governance/model-assets/df-compatible-release-asset-v1/legal-review.json --approval governance/model-assets/df-compatible-release-asset-v1/approval-manifest.json --asset vendor/approved/df-compatible-release-asset-v1.bin --approver-key governance/model-assets/df-compatible-release-asset-v1/approver-public-key.pem --status-out docs/evidence/m0-asset-gate.json
```

The expected result is exit code 0 and `docs/evidence/m0-asset-gate.json` containing `M0_APPROVED` plus the verified asset, record, and key hashes. The old incomplete fixture command remains a negative security test and must report `BLOCKED_NO_APPROVED_ASSET`.

## External approval boundary

Production records under `governance/model-assets/df-compatible-release-asset-v1/` and the matching blob at `vendor/approved/df-compatible-release-asset-v1.bin` are authorized by the project owner's explicit license directive for this free and open-source application. João Messias Lima Pereira recorded that directive as reviewer and approver; it is not external legal counsel. The private approval key stays outside the repository. No alternate asset, ambiguous DeepFilterNet weight, automatic download, or inferred license conclusion is allowed.

## Technical candidate selection (not approval)

The user selected the standard `DeepFilterNet3_onnx.tar.gz` from official `Rikorose/DeepFilterNet` tag `v0.5.6` as a technical candidate. This explicitly excludes `DeepFilterNet3_ll_onnx.tar.gz` and any alleged official Large model. The upstream identity, content hash, and listing-only archive inspection are recorded in `docs/evidence/dfn3-standard-v0.5.6-technical-candidate.json`. This historical technical-selection evidence is now `SUPERSEDED_BY_PRODUCTION_APPROVAL`: it does not itself infer a weight license, replace the signed production dossier, authorize a key or signature, approve release, or unblock M0.

The committed templates use `status: PENDING`, empty values, and no signature. The fixed production trust policy authorizes exactly `sha256:cfa7e10c021031f5481775cf32093d41e5aaaad454653f3ed0022769cbbfd3f2`; the incomplete fixture key and all ephemeral test keys remain untrusted. Tests generate an ephemeral keypair and inject its key ID only into pure-function verification in a temporary directory. The production CLI never accepts a caller-selected policy.

The aggregate manifest references child schemas by their declared `$id`. Offline validation must preload the local files under `governance/model-assets/schemas/` into an ID catalog; no network lookup is required or permitted.

## Downstream block

While M0 is `BLOCKED_NO_APPROVED_ASSET`, M1 and Tasks 2-17 are blocked. `M0_APPROVED` authorizes the next planned task; it does not itself approve GA.
