# Release partition-capacity sweep — 2026-08-08

Commit `3d823f8da34c9ac1bceb543febf7359ac4ad26e3` was built with `--release` and measured on the
long-term host (8 logical CPUs). The database-free workload uses the production partition queue,
S2 routing, immutable worker results, complete barrier validation, event budgets, and canonical
schedule checkpoints. Each active synthetic durable individual emits one event per tick. Wall time
is operational evidence only and never enters a digest or world state.

The exact command was:

```bash
target/release/civilization-runner capacity-sweep \
  --populations 66,660,6600,66000 \
  --active-percents 1,10,100 \
  --ticks 64
```

| Population | Active | Active lives | Ticks/s | Events/s | Canonical event bytes |
|---:|---:|---:|---:|---:|---:|
| 66 | 1% | 1 | 82,873 | 82,872 | 8,422 |
| 66 | 10% | 7 | 53,334 | 373,343 | 58,954 |
| 66 | 100% | 66 | 12,306 | 812,258 | 555,852 |
| 660 | 1% | 7 | 8,232 | 57,627 | 58,954 |
| 660 | 10% | 66 | 5,731 | 378,270 | 555,852 |
| 660 | 100% | 660 | 1,325 | 874,841 | 5,558,520 |
| 6,600 | 1% | 66 | 610 | 40,314 | 555,852 |
| 6,600 | 10% | 660 | 502 | 331,688 | 5,558,520 |
| 6,600 | 100% | 6,600 | 127 | 842,006 | 55,585,200 |
| 66,000 | 1% | 660 | 55 | 36,607 | 5,558,520 |
| 66,000 | 10% | 6,600 | 42 | 278,648 | 55,585,200 |
| 66,000 | 100% | 66,000 | 12 | 819,181 | 555,852,000 |

The 66,000-person fully active sample resolved 4,224,000 events in 5.156 seconds. Its event-stream
digest was `721823f78f1c633999274d7d281fca6c737e8aacdd4a92116867839b50915974` and its final schedule
digest was `1ae9c3a8c3eeefc730c9b6c00958fb41dd3572bdda45c8d339baec8891f0e6b8`.

This is scheduler-kernel capacity evidence, not a claim that 66,000 fully embodied ruleset-30
lives—including PostgreSQL persistence, projections, memory, and cognition—advance at 12 ticks per
second. The fresh v19 integrated qualification separately measures the real 66-founder ruleset-30
path at tick 1,000. Together they establish initial-genesis headroom and deterministic scale
behavior without claiming a twenty-billion-person production envelope.
