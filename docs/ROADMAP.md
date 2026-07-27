# Roadmap

[简体中文](zh-CN/ROADMAP.md)

These are candidates, not promises:

1. Evidence-preserving, budgeted pairwise scenario reduction, because interactions can reveal assumptions that single changes do not.
2. Stronger cross-platform descendant-process termination, balanced against portability and unsafe-code avoidance.
3. Optional file-access evidence with an explicit privacy model, to narrow `EMPTY_HOME` findings without reading unrelated home content.
4. Capability-detected copy-on-write acceleration with integrity fallback tests for large workspaces.
5. More deterministic oracles, including optional file SHA-256 checks and normalized structured output.

No roadmap item changes the security boundary: AssumeZero remains a testing tool for trusted finite commands, not an untrusted-code sandbox.
