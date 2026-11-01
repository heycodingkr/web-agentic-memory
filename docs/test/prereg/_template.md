# Preregistration: <date> / <suite>

Status: draft. Complete and commit before collecting the measured runs. Record the resulting commit in evidence; changing criteria after seeing results requires a new preregistration.

## Hypothesis and primary metrics

- Hypothesis:
- Primary metrics: time from first tool call to submit; tool calls through submit.
- Claim rule: at least 20% median reduction and positive 95% bootstrap CI (10,000 samples), success rate not lower, average errors not higher.
- Other metrics are exploratory and cannot replace the primary metrics after measurement.

## Fixed inputs

- Host / version / model / effort:
- Prompt hash / tool definition hash / memory version IDs:
- Tasks, fixture/UI versions and held-out split:
- Independent oracle and success/failure definitions:
- Fresh HOME/config/profile/cache setup:
- Plan path, SHA-256, crossover seed, conditions and repetitions:

## Exclusions and recovery

- Use the documented invalid signal classes; retain every run and its reason.
- More than 20% invalid makes the whole suite invalid.
- Valid failures before submit retain null time/calls and stay in success-rate denominators.
- Stop conditions, timeouts and retry policy:
- Host/browser guard interception and cleanup evidence:

## External runs

- Dedicated test account and verified service automation conditions:
- Invocation/token/credit budget and maximum run count:
- Unavailable features, other limitations and uncontrolled factors:

## Reporting

- Evidence root:
- Common metadata and preregistration commit:
- Report the registered conditions with success/failure/invalid counts, medians, mean/SD, CI and grade.
- Scripted fixtures and offline replay are harness QA, not live agent-performance evidence.
