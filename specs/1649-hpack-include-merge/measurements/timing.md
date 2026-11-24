# T020: timing (SC-007, within 1.05× `main`)

`timing.py` times wall-clock per scan of each committed copy:
- `--offline sbom scan --no-deep-hash`, CycloneDX, with `HOME` isolated;
- the two arms interleaved, alternating which goes first each round;
- both arms are debug builds (`control.md`), on the same host, in one session.

## 3 runs per arm (the task)

| Copy | main min / median / max (s) | branch min / median / max (s) | median ratio |
|---|---|---|---|
| hs-opentelemetry | 0.624 / 0.626 / 0.769 | 0.622 / 0.630 / 0.727 | 1.006 |
| lootbox | 0.096 / 0.099 / 0.119 | 0.103 / 0.104 / 0.105 | 1.044 |
| hs-temporal-sdk | 0.277 / 0.285 / 0.313 | 0.284 / 0.288 / 0.288 | 1.010 |
| ogmios | 0.970 / 0.979 / 1.090 | 0.978 / 0.981 / 1.000 | 1.002 |

## 7 runs per arm

Repeated because lootbox's 1.044 sat close to the bound, on a difference of about 5 ms in a 100 ms scan:

| Copy | main min / median / max (s) | branch min / median / max (s) | median ratio |
|---|---|---|---|
| hs-opentelemetry | 0.610 / 0.618 / 0.674 | 0.611 / 0.623 / 0.701 | 1.008 |
| lootbox | 0.098 / 0.100 / 0.126 | 0.101 / 0.103 / 0.139 | 1.027 |
| hs-temporal-sdk | 0.277 / 0.284 / 0.321 | 0.283 / 0.285 / 0.332 | 1.003 |
| ogmios | 0.966 / 0.979 / 1.032 | 0.976 / 0.989 / 1.069 | 1.010 |

**SC-007 holds:** every median ratio is at most 1.044 at 3 runs and at most 1.027 at 7 runs. The added work is linear in each manifest's text, plus one read per include (49 on the corpus), and it is skipped entirely for a manifest with neither `!include` nor `<<`.
