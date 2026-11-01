# Contract: the classification rule

One rule, implemented twice because the evidence arrives differently. Stated
here so a disagreement between the two implementations is a contract
violation rather than a matter of opinion (FR-007).

## Inputs

| reader | development evidence | runtime/optional evidence | edges for the closure |
|---|---|---|---|
| bun | `workspaces.<path>.devDependencies`, every path | same map's `dependencies` and `optionalDependencies` | the `packages` map's per-entry dependency objects |
| yarn | `devDependencies` of the root manifest and of each discovered member manifest | the same manifests' `dependencies` and `optionalDependencies` | the lockfile's own per-entry `dependencies` |

## The rule

1. For every workspace, resolve each declaration to a package identity under
   the reader's existing normalization.
2. Discard any identity the lockfile's package map does not contain
   (FR-012). No identity is synthesised. This is what excludes
   `workspace:`, `link:`, `portal:` and `file:` declarations.
3. Partition the surviving identities: those declared under
   `devDependencies` into the development set, those declared under
   `dependencies` or `optionalDependencies` into the protected set. A
   declaration appearing in both lands in protected.
4. Transitively close the protected set over the lockfile's edges.
5. Subtract the closed protected set from the development set.
6. Assign `LifecycleScope::Development` to what remains, and to nothing else.

## Invariants

- **Disjoint.** The development and protected sets share no identity after
  step 5. Asserted by test, not by argument.
- **Runtime wins, globally.** An identity reachable from any workspace's
  runtime or optional declarations is never development, whatever another
  workspace calls it (FR-002). This is the Principle IX protection.
- **Optional is never demoted.** No identity that would be assigned
  `Optional` today becomes `Development` (FR-003).
- **Order matters and is fixed.** Subtraction follows closure. Subtracting
  before closing would mark a runtime-reachable transitive as development and
  milestone 1269's propagation would then spread that error.
- **No new identity.** Step 2 cannot add a key the lockfile lacks.
- **Empty is normal.** A manifest or workspace map with no development
  declarations yields an empty development set, which is not a degradation
  and is silent.

## Non-goals

Edge direction, relationship types and component emission are unchanged. This
contract assigns a scope to components the readers already produce; it does
not add, remove or re-link any of them.
