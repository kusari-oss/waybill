# Corpus golden review (SC-007, research R13)

Run 43676912729 (`regen_goldens=true`, branch `1530-therock-capture` at `2562e5c0`, whose source is identical to `1530-git-submodule-reader`). Every target regenerated: 22 "goldens written" lines, 21 `test corpus_* ... ok`.

**Byte comparison.** The 67 regenerated files were compared with the committed goldens. **3 differ, exactly as predicted**: `public_corpus/haskell-language-server/{cdx,spdx-2.3,spdx-3}.json`. The other 64 are byte-identical, and no file is new.

**Content of the 3.** A sorted-key `jq -S` diff of each shows only added lines, and they are the C222 record:

```
{"pinned":0,"skipped":{"git-refused":0,"git-unavailable":0,"no-gitlink":0,"no-repository-data":0,"no-url":0,"unsafe-path-or-url":0}}
```

The record appears as a CycloneDX `metadata.properties[]` entry, an SPDX 2.3 document annotation and an SPDX 3 `Annotation` on the document. haskell-language-server has a comment-only `.gitmodules` at its pin and no gitlinks (research R13), so the record says "seen, nothing pinned, nothing skipped". No component and no edge changed in any golden.
