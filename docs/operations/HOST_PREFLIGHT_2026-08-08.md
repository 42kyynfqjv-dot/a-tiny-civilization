# Production-host public-genesis preflight — 2026-08-08

The project host passed the complete read-only public-genesis preflight for world
`b3ea736d-7a5a-5161-a74b-fa8c4302d333`. No canonical world was activated, no service was
changed, and no site was deployed.

The successful host run verified:

- the root-owned mode-0600 production environment, with local keyless cognition and the explicitly
  deferred offsite-backup state;
- launch-candidate evidence through tick 1,000 / sequence 1,018 at source commit
  `0d9d60619b7d6d762aa55bab86beb97f3eec3d6a`;
- the seven-dimension experimental quality-world admission;
- the six-route source-bound public-observatory admission at commit
  `d5e1c209fe1b490c040b509d90287137c311f9c9` (superseding the initially verified
  `cf5f045be50d71efb223c58a877682febe7cd866` tree after explicit non-root container ownership was
  added);
- a fresh composition-0.1.1 runtime tree containing 147,469 listed files, owned by root and the
  runner service group with no group/world write access; and
- complete full-Earth composition validation plus exact length and SHA-256 verification of both
  JPL DE441 segments.

Two obsolete generated composition-0.1.0 staging trees (about 3.9 GB total), 3.5 GB of unused
Docker build cache, and 18.23 GB of images unused by every container were removed before the final
run. These were pull/build/stage-recoverable copies; retained source data, active images and
containers, database volumes, genesis artifacts, and qualification evidence were not removed.
Every active container remained running and its previously declared health state was unchanged.
Free filesystem capacity rose above the checked-in 10 GiB backend health floor.

The corrected candidate images were then built locally without changing any running service. The
backend image ID is `sha256:183ad1363f901234766456aa8ad18dc285af995c3849b0a5c0a932842fcd8e0f`
and declares runtime user `civilization`; the observatory image ID is
`sha256:81982273b92dbe6eade352704d4a8384dc7159aa40b643ac0e110379c1cec700` and declares runtime user
`node`. Both passed the checked-in smoke test with a read-only root filesystem, all capabilities
dropped, no-new-privileges, and only a temporary loopback port for the web process. The smoke caught
and corrected owner-private checkout modes before admission was renewed. The final full host
preflight then passed again against the replacement observatory admission. About 23 GB remained
free after both image builds.

The remaining external configuration is deliberately outside this evidence: legal operator and
jurisdiction review, monitored policy mailboxes, and optional Google/Apple/Stripe activation. Those
items gate accounts and payments, not a free anonymous observatory. The qualified local
Qwen/Hindsight path requires none of those integrations. Deployment and world activation remain
separate literal-confirmation operations.

The empty ownership-labelled production volumes now exist as
`a-tiny-civilization-postgres-v1`, `a-tiny-civilization-hindsight-v1`, and
`a-tiny-civilization-hindsight-model-cache-v1`. The live development world at tick 139,382 is in
the distinct legacy `emergent-civilization_postgres-data` volume. Development currently owns
loopback ports 3000, 5432, and 8080; the deployment helper now rejects that state before mutation
and requires a deliberate service cutover without deleting the legacy volume.
It also rejects the legacy local-cognition container while that container holds the shared
`atiny-ollama` model volume, preventing two Ollama processes from sharing its writable state.
