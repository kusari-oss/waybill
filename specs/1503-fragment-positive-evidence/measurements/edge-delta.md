# Edge and role delta across the five closures (FR-007, SC-001 to SC-004)

Per-target detail is in `<target>/delta.md`. Captures: public-corpus run 43629708718. The `edges` column is the C184 figure (`projected, identity level`).

| target | leave `fragment` | → `scaffolding` | → `package` (setup-hook) | → `package` (default) | `fragment` after | walked-through Δ | edges before | edges after | Δ |
|---|---|---|---|---|---|---|---|---|---|
| moat | 25 | 10 | 0 | 15 | 215 | -581 | 6,891 | 7,489 | +598 |
| shellcheck | 26 | 11 | 0 | 15 | 215 | -1,038 | 10,224 | 11,274 | +1,050 |
| slack-web | 66 | 3 | 36 | 27 | 0 | -3,090 | 44,426 | 8,956 | -35,470 |
| openai | 67 | 3 | 37 | 27 | 0 | -3,150 | 48,336 | 9,478 | -38,858 |
| slacklinker | 35 | 15 | 0 | 20 | 215 | -1,238 | 12,169 | 13,416 | +1,247 |
