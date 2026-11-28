## M1 freeze ignores members
exit=100
        FAIL [   0.163s] ( 33/229) waybill::haskell_freeze_members freeze_governs_every_packages_form
        FAIL [   0.160s] ( 34/229) waybill::haskell_freeze_members freeze_governs_subdirectory_member
        FAIL [   0.185s] ( 35/229) waybill::haskell_freeze_members unlisted_directory_is_not_governed
        FAIL [   0.344s] ( 48/229) waybill::haskell_freeze_members unpinned_member_dep_is_unchanged
     Summary [   2.923s] 229 tests run: 225 passed, 4 failed, 8764 skipped
        FAIL [   0.163s] ( 33/229) waybill::haskell_freeze_members freeze_governs_every_packages_form
        FAIL [   0.160s] ( 34/229) waybill::haskell_freeze_members freeze_governs_subdirectory_member
        FAIL [   0.185s] ( 35/229) waybill::haskell_freeze_members unlisted_directory_is_not_governed
        FAIL [   0.344s] ( 48/229) waybill::haskell_freeze_members unpinned_member_dep_is_unchanged

## M2 no glob matching
exit=100
        FAIL [   0.458s] ( 41/229) waybill::haskell_freeze_members freeze_governs_every_packages_form
        FAIL [   0.090s] ( 62/229) waybill::project_discovery_haskell us2_f4_cabal_project_members_join_json_root
        FAIL [   0.017s] (109/229) waybill::bin/waybill scan_fs::package_db::haskell::root_declared_members_tests::a_directory_glob
        FAIL [   0.015s] (113/229) waybill::bin/waybill scan_fs::package_db::haskell::root_declared_members_tests::cabal_file_globs_name_their_directory
        FAIL [   0.014s] (126/229) waybill::bin/waybill scan_fs::package_db::haskell::s1668_governed_dirs_tests::cabal_project_members_reads_every_packages_form
        FAIL [   0.016s] (127/229) waybill::bin/waybill scan_fs::package_db::haskell::root_declared_members_tests::summary_is_some_whenever_a_root_declaration_resolves
     Summary [   2.907s] 229 tests run: 223 passed, 6 failed, 8764 skipped
        FAIL [   0.458s] ( 41/229) waybill::haskell_freeze_members freeze_governs_every_packages_form
        FAIL [   0.090s] ( 62/229) waybill::project_discovery_haskell us2_f4_cabal_project_members_join_json_root
        FAIL [   0.017s] (109/229) waybill::bin/waybill scan_fs::package_db::haskell::root_declared_members_tests::a_directory_glob
        FAIL [   0.015s] (113/229) waybill::bin/waybill scan_fs::package_db::haskell::root_declared_members_tests::cabal_file_globs_name_their_directory
        FAIL [   0.014s] (126/229) waybill::bin/waybill scan_fs::package_db::haskell::s1668_governed_dirs_tests::cabal_project_members_reads_every_packages_form
        FAIL [   0.016s] (127/229) waybill::bin/waybill scan_fs::package_db::haskell::root_declared_members_tests::summary_is_some_whenever_a_root_declaration_resolves

## M3 .cabal entry not its dir
exit=100
        FAIL [   0.350s] ( 35/229) waybill::haskell_freeze_members freeze_governs_every_packages_form
        FAIL [   0.016s] (114/229) waybill::bin/waybill scan_fs::package_db::haskell::root_declared_members_tests::cabal_file_globs_name_their_directory
        FAIL [   0.015s] (127/229) waybill::bin/waybill scan_fs::package_db::haskell::s1668_governed_dirs_tests::cabal_project_members_reads_every_packages_form
     Summary [   3.182s] 229 tests run: 226 passed, 3 failed, 8764 skipped
        FAIL [   0.350s] ( 35/229) waybill::haskell_freeze_members freeze_governs_every_packages_form
        FAIL [   0.016s] (114/229) waybill::bin/waybill scan_fs::package_db::haskell::root_declared_members_tests::cabal_file_globs_name_their_directory
        FAIL [   0.015s] (127/229) waybill::bin/waybill scan_fs::package_db::haskell::s1668_governed_dirs_tests::cabal_project_members_reads_every_packages_form

## M4 lock dir not governed
exit=100
        FAIL [   0.013s] (127/229) waybill::bin/waybill scan_fs::package_db::haskell::s1668_governed_dirs_tests::members_are_canonical_and_unique
        FAIL [   0.012s] (128/229) waybill::bin/waybill scan_fs::package_db::haskell::s1668_governed_dirs_tests::no_members_governs_only_the_lock_dir
     Summary [   2.930s] 229 tests run: 227 passed, 2 failed, 8764 skipped
        FAIL [   0.013s] (127/229) waybill::bin/waybill scan_fs::package_db::haskell::s1668_governed_dirs_tests::members_are_canonical_and_unique
        FAIL [   0.012s] (128/229) waybill::bin/waybill scan_fs::package_db::haskell::s1668_governed_dirs_tests::no_members_governs_only_the_lock_dir

## control (unmutated)
exit=0
     Summary [   2.986s] 229 tests run: 229 passed, 8764 skipped

| mutation | exit | killed |
|---|---|---|
| M1 freeze ignores members | 100 | yes |
| M2 no glob matching | 100 | yes |
| M3 .cabal entry not its dir | 100 | yes |
| M4 lock dir not governed | 100 | yes |
| control | 0 | n/a |
