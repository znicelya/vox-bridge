# Task5C pre-start owned-handler fixture ruling — 2026-11-07

SystematicDebugging evidence: existing AppState ownership fixtures register a
blocking Reindex handler after AppState construction. Production Task5C now
correctly registers the real Reindex handler during composition, so the old
test fails at register with Intelligence(Configuration), before its ownership
assertions run. `task-5c-owned-registration-red.log` is runtime0pass/1fail,
not a failed implementation of close ownership.

Do not weaken production register-once/freeze semantics or move these fixtures
to a temporarily unused Research/Summary kind. Tasks6/7 will register every
remaining production kind, causing the same collision again; changing kind
also changes the document exclusion contract the original fixtures exercised.

Accepted narrow fixture seam, compiled only under cfg(test):
`intelligence::jobs::test_support::replace_pre_start_handler(&JobScheduler,
JobKind, Arc<dyn JobHandler>)`. A child module may inspect private State; it
requires frozen=false, generation=None and the handler already present,
otherwise returns Configuration without mutation. It replaces the registered
handler only for a pre-first-start fixture. It cannot replace during execution,
after shutdown or on reopen. Production register and composition are unchanged.

Keep original Reindex kinds, document exclusion keys, blocked cleanup/cancel/
actual process-owner assertions and real scheduler lifecycle. Verify these
existing tests after the fixture adaptation; a narrow boundary test may prove
replacement after first start is rejected. Do not replace/copy scheduler
execution logic or add a new production JobKind for testing.

Cost if wrong: one private test-only injection helper and a small fixture
adaptation; it does not ship or change a production transaction/handler. The
review must verify cfg(test), the initial-state guard and preserved assertions.
