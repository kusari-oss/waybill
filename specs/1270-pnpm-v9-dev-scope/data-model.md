# Data Model

The importer helper returns development seeds and the non-development protected closure. It unions runtime and optional declarations across importers, walks the snapshot graph once, then removes every protected identity from the development set. Missing or empty evidence returns None and preserves legacy flags.

Protected closure entries retain explicit Runtime when the existing optional/peer classifier has not assigned their scope. This preserves the previous default for unmarked optional paths and prevents the global additive pass from demoting them when a child manifest is absent. Existing optional/peer markers retain precedence. Only remaining unclassified transitives receive None and may acquire scope from the existing pass. Snapshot edges sharing an emitted identity are unioned across peer contexts. No serialized data model changes.
