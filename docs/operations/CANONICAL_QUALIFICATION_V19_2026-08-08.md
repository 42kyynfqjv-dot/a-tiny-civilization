# Canonical qualification v19 evidence — 2026-08-08

This source-renewal run proves the unchanged ruleset-30 candidate against commit
`3d823f8da34c9ac1bceb543febf7359ac4ad26e3`, after adding operational capacity probes and stronger
partition disruption tests. It is mechanical qualification evidence, not scientific admission or
permission to deploy.

The fresh isolated PostgreSQL database independently traversed all 147,466 provisional references
(10,164,215,509 bytes). Tick zero reproduced sequence `1`, event head
`046f3aebc6352e94ab75e9d0baecdeca5005034319245bc2c4d033ce96fafc34`, and state hash
`c77534f6431c0d0e6a88290f4beeae84e86a4d71c81988b8508a5f7c58d23911`.

At tick 1,000 the world reached sequence `1018` and state hash
`2045f73ef3a5601b257de06d1862c9515a1ff2e4540b32d7762a7288ea7e06ff`, exactly matching v18.
The event head is `d719c546c93adf652ddfb6f5806849f9f0aa83c13f4a29692be24b0ef8282e02`;
it may differ between qualifications because local-model results are nondeterministic recorded
inputs, while replay consumes only the retained result. Genesis replay and snapshot-plus-tail
resume both matched the committed cursor.

All 4,000 Hindsight memory deliveries completed with zero pending or errors. Seventeen person-only
cognition requests existed; all 16 due requests were latched and consumed, two zero-cost local
Qwen receipts were recorded, and seven results included recalled memory. No remote provider or paid
route was enabled. All five observer projections reached sequence `1018`, exposing 66 organisms,
67 safe timeline items, eight deterministic findings, and 23 region-bound neutral artifact traces.
Every schema-1 qualification check passed.

The immutable bundle is retained outside Git at
`a-tiny-civilization-qualification-evidence/b3ea736d-7a5a-5161-a74b-fa8c4302d333-ruleset30-v19-tick1000`.
Its `SHA256SUMS` digest is
`0ccee506f338d05494834c11ef936e9205676fa14694b2eed3adf5b816a64155`; the qualification report
digest is `e48eeb9750954d753ab2a9f2093e03ac1cd5a2b5f1138b3ee2c17def35f73ce1`. The bundle declares that
it contains no canonical event payloads.
