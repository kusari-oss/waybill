# Corpus goldens (T016, SC-003)

`public-corpus.yml` with `regen_goldens=true`: run 43792910635.
- **Scanned commit:** `f9c750610e77580ed33c5aaca6f1785920c8ba41`, the branch head, as the summary's "Scanned commit" line reports. The workflow definition came from `96a7ef32`, `main`.
- **Comparison:** all 66 regenerated files were byte-compared (`cmp`) with the committed goldens. **0 differ.**

The prediction (plan step 9) held:
- no corpus target has a versioned installed-library component its binaries' paths name;
- bundled-tree detection, which runs on every binary scan, found no binary in the public corpus naming a vendoring directory.
