# Trace

## T001: premises at `main` (`52b96f1d`)

- **(a) The name and ancestor rules miss the shapes.** Shown by tests written before any source change, run against `main`'s code:
  - `m670_scope_classify_filename_signals`, extended: `requirements-doc.txt classified as Main, expected Optional("docs")`;
  - `s1613_the_selector_reads_labelled_names_only`, extended: `doc-requirements.txt` not read;
  - `s1615_nearest_docs_or_test_ancestor_labels_the_file`: `p/docs/sphinx/requirements.txt` gives no scope.

  In the integration suite, 8 cases fail on `main` (W-1/2, W-3, W-4, W-5, W-7, W-8, W-10's docs package, W-13), each on the label assertion. 6 pass as guards (W-6, W-9, W-11, W-12, W-14, W-rep). W-ctl's golden was generated at this point.
- **(b) `listed_ancestor_class` has three callers:** `classify_requirements_scope_under` and `is_requirements_file` in `requirements_txt.rs`, and `ci_scope::class_of` (spec 1613 R5, every ecosystem). The other hits for it are its own unit tests in `project_roots.rs`.
- **(c) `ci_scope::apply` removes C155 from any component whose final scope is runtime:** `scan_fs/ci_scope.rs`, the `extra_annotations.remove(ANN_REQ_FILE_SCOPE)` branch, with its unit test asserting the removal.
