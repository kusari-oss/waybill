# Data model: CMake conditional scope

No emitted field changes shape. C122, C232, C236 and the native scope keep their forms. Only which components carry which scope changes.

## Evaluation (internal)

| Value | Meaning |
|---|---|
| `Holds` | the condition is true in the default configuration |
| `DoesNotHold` | the condition is false in the default configuration |
| `Unknown` | the reader cannot tell |
| `Platform` | true on some supported platform; within a chain it acts as `Holds` (research R2) |

## Branch (internal, research R3)

One enclosing `if` block: the earlier branch conditions, plus the current one with its kind (`if`, `elseif` or `else`). Kept beside, not instead of, the C232 string.

## Default (internal, research R4)

| Field | Meaning |
|---|---|
| name | the option or variable |
| value | `Holds`, `DoesNotHold` or `Unknown` |
| source | `option`, `cmake_dependent_option`, `cache` or `set` |
| file | where it is declared |

Several in reach with different values make the value `Unknown`.

## Scope outcome (emitted)

| Chain evaluation | Fetch | Package search's library |
|---|---|---|
| `DoesNotHold` | Optional, C122 `cmake-conditional`, C232 | Optional when every search is `DoesNotHold`; C236 |
| `Holds` / `Platform` / `Unknown` | no scope, no C122, C232 | runtime (in scope); C236 |

The six specific shapes and Test are not re-evaluated.

## Census event (log only, research R6)

`target: waybill::cmake_conditions`, with the fields `kind`, `subject`, `versioned`, `category`, `eval` and `conditions`.
