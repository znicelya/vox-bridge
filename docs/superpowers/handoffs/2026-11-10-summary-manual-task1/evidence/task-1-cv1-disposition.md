# CV-1 controller disposition

The measured 200,000-row Native summary history returned Timeout at
10.000312414 seconds; successful near-ceiling recovery was not established.
The test's one pass verifies the observed fail-closed result and joined
resource cleanup. It does not certify availability at that history size.

Root read the independent Spec PASS / Quality Approved review and complete
validation report. The latter records exactly one measurement, all 200,000
coherent persisted tuples and raw payload witnesses, closed readiness, zero
provider calls, and actual joined cleanup. Return observations interleaved
with destruction; a live strong owner at Timeout return is not claimed.
The original strict audit exit1 is retained; the separate audit exit0 allows
only Root's expressly attributed 448-byte progress-file append.

Ruling: Accept the manual deliverable under its binding fail-closed timeout
contract and carry the measured near-ceiling availability limitation into
Task2 and final review, without claiming this history size normally recovers
— the brief/interface amendment explicitly allows observed recovery timeout
with readiness fenced, and the checkpoint startup section preserves durable
uncertainty on oversized/deadline-limited history; no successful ceiling
throughput guarantee is bound — cost if wrong: sufficiently large histories
cannot open intelligence at the current10s recovery observation and require
a scalable recovery change before that capacity can be advertised as usable.

Task2 must preserve closed startup/readiness on this result, retain and join
actual pending work, and perform no paid replay. Its final integration review
receives this limitation. The observation used the accepted local unoptimized
Cargo test environment and synthetic interrupted history; production release
throughput and successful ceiling recovery remain unmeasured.

Evidence: [independent review](task-1-independent-review.md),
[validation report](task-1-cv1-validation-report.md),
[actual measurement](task-1-cv1-validation/measurement.json),
[strict original audit](task-1-cv1-validation/post-validation-audit.json), and
[final preservation audit](task-1-cv1-validation/final-preservation-audit.json).
