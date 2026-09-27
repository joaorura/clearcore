# M1 Qualification Corpus Governance

No qualification audio is committed, and no local corpus manifest is currently approved. This keeps M1 at `BLOCKED_PENDING_GOLDEN`.

Before `generate-golden` may read a local item, `fixtures/corpus/corpus-manifest.json` must bind each stable case ID to an immutable local origin, SHA-256, license, redistribution terms, consent or processing authorization where applicable, transcription applicability, SNR/noise class, sample rate, channel count, and ordered frame count. The manifest itself must be checksummed and retained with the isolated generation evidence.

The approved corpus must cover adult male and female voices, authorized child speech when used, stationary noise, keyboard, traffic, fan, music, competing speech, reverberation, declared SNR levels, and silence. Every item must be mono 48 kHz PCM normalized by a frozen procedure before 480-sample framing. Model approval does not approve corpus material, and corpus approval does not approve the model.

Corpus audio remains local, is never uploaded by the product, and is not added to this repository. Missing provenance, authorization, coverage, format, or checksum is a block rather than a skipped case.
