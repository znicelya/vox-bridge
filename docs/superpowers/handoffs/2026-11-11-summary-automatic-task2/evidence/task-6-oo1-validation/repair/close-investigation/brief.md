# Read-only close-interruption root-cause investigation

Root is the sole author/Cargo owner, currently running Repair034. This is a
targeted debugging read, not source acceptance or a replacement final review.
No source/Cargo/index/ref/global change, agent dispatch or additional test.

Read `inputs.json`, `source.diff` and the eleven immutable candidate snapshots.
They cover the actual SQL, scheduler generation, maintenance, delivery,
session/runtime and App close producers. Base is the accepted pre-repair tree
d1dd2c7. If precise unchanged context is necessary, read that tree's specific
producer rather than the entire plan/source. Root continues local work.

The required ownership/limits are in `../../oo1-repair-brief.md`. Real joins,
two workers, one SQL slot per worker/maintenance/observer/delivery, two actor
handles and same-key native/child/lease retention remain binding. No status or
semaphore availability can prove an actual task has ended.

Observed failures: Repair018 had subscription deletion-retry close return
Internal("Intelligence work was interrupted."); Repair030 had the same
close error in summary_app_cancelled_issued_ready_observer_preserves_actual_
publication_and_repeated_startup. Focused runs022/024/025/029 and full026 passed
the first case, and029 passed both. Repair026's sole failure was a32MiB fixture's
5s guard (isolated4.27s); its bounded test-only wait is now30s. Do not conflate
that guard with the unexplained close error or a product deadline.

Use the immutable raw logs/JSON records in the parent repair directory.
Repair034 adds only cfg(test) diagnostics for failed App close branches and
the scheduler's actual shutdown join result. Root owns this running Cargo;
you may read completed log output, never modify it or start another process.

Identify a concrete source race/failure path if supported. Distinguish an
expired observation deadline from abnormal generation completion, delivery
failure and actual ownership loss; do not guess from the generic error text.
Recommend the smallest named regression/repair if one is justified. If source
and evidence cannot attribute it, state exactly what is missing. No approval
verdict or test pass extrapolation is requested.

Write only `close-investigation/report.md` beside this brief, with findings,
source/evidence references, input integrity and explicit no-pending-work
handback. Return a concise finding and report hash. The original Task6 whole
review and Task8A review remain unchanged; final scoped repair review is later.
