# Task2 same-key close retry outcome ruling

This is a Root decision during the existing authorized Task2 execution, not
a new source acceptance or permission to enter a new native generation.

## Ruling

A later close attempt may perform a fresh bounded result pass for a
recoverable persistence, jobs, subscriptions or diagnostic failure while
reusing the original elected key, delivery UUID and genuine actual audio
acknowledgment. The first failed attempt must still return its error. Keep
that failure in the evidence/result history; the actual receipt does not
turn that failed attempt into a successful stop result.

The retry may report successful close only when the previously failed
operation has actually succeeded, or a focused current-state check proves
its required durable/barrier effect is complete, and all current required
close branches and barriers are complete for that same key. A new
`stop_active()` finding no active actor, a cached terminal status, or the
actual ack alone does not prove a failed lifecycle/diagnostic write was
completed. If there is no genuine retry or verification producer for the
failed effect, retain the error and keep rearm fenced.

Retain the same actual close source/owner through observation timeout and
retry. Do not create a new key/UUID, accept a newer generation's receipt, or
perform a replacement open/inventory/poll while Closing. Drain and join
delivery and jobs before the direct16/index barrier on every successful
path. An error in one branch cannot abandon the other actual owners.

Native operation failures and sticky quarantine/NeedsRestart retain their
own policy. Receipt of actual exit cannot clear them. A later successful
transient-result pass does not reset native policy or invent a successful
native stop. Any host reopen still requires the completed same-key close
and the independent backend/health policy checks.

## Basis and cost

The accepted pre-Task2 `app/lifecycle.rs` at BASE85eda3a explicitly permits
retry after failed close and performs a new result pass; its old fresh-key
election is superseded by the same-key Task2 interfaces. Binding amendments
lines228–241 require retained same-key actual proof and explicitly separate
that receipt from diagnostic/storage/result errors and sticky native health.
Lines261–266 require both actual branches before retirement; they do not
require all later attempts to repeat a corrected transient error forever.

Cost if wrong: a retry implementation that merely drops the old error could
allow a window to reopen with an incomplete durable lifecycle or diagnostic
effect. Therefore corrected-effect verification and sticky native health
are required, and an unverified old failure stays fenced.

## Focused covering evidence

Exercise a current failed attempt with genuine same-key actual ack and
recoverable injected persistence/diagnostic failure. Its result is Err and
rearm stays closed. With that fault corrected, a real retry/completeness
check may succeed without changing key/UUID or starting native work. Also
exercise a still-uncorrected effect, an actual ack with NeedsRestart, and a
no-active-only retry: none may erase the unresolved failure or native policy.
The report names the production producer/check used to establish recovery.
