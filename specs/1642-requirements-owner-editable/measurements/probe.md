# Research probe at `main` `79b3350b` (R1, R4)

Run: `measurements/probe.sh target/debug/waybill <scratch>`. The debug binary was built from `79b3350b`; the branch had changed only `specs/` when it ran. Each R4 variant was scanned three times.

```
R1 waybill-fixture-devtool uv.lock:with | edges into shared: pkg:pypi/waybill-fixture-devtool@0.1.0, <scan root>@0.0.0 | waybill-fixture-devtool: waybill:source-files=["tool/uv.lock"] waybill:workspace-member=["tool"]
R1 waybill-fixture-devtool uv.lock:without | edges into shared: pkg:pypi/waybill-fixture-tool@1.0.0 | waybill-fixture-devtool: waybill:source-files=["tool/devtool"] waybill:workspace-member=["tool/devtool"]
R1 waybill-fixture-zdevtool uv.lock:with | edges into shared: pkg:pypi/waybill-fixture-tool@1.0.0, <scan root>@0.0.0 | waybill-fixture-zdevtool: waybill:source-files=["tool/uv.lock"] waybill:workspace-member=["tool"]
R1 waybill-fixture-zdevtool uv.lock:without | edges into shared: pkg:pypi/waybill-fixture-tool@1.0.0 | waybill-fixture-zdevtool: waybill:source-files=["tool/devtool"] waybill:workspace-member=["tool/devtool"]
r4 run 1 | root: pkg:pypi/waybill-fixture-app@1.0.0 | edges into shared: pkg:pypi/waybill-fixture-app@1.0.0, pkg:pypi/waybill-fixture-devlib@1.0 | shared: excluded waybill:lifecycle-scope=optional waybill:lifecycle-scope-derivation=transitive
r4 run 2 | root: pkg:pypi/waybill-fixture-app@1.0.0 | edges into shared: pkg:pypi/waybill-fixture-app@1.0.0, pkg:pypi/waybill-fixture-devlib@1.0 | shared: excluded waybill:lifecycle-scope=optional waybill:lifecycle-scope-derivation=transitive
r4 run 3 | root: pkg:pypi/waybill-fixture-app@1.0.0 | edges into shared: pkg:pypi/waybill-fixture-app@1.0.0, pkg:pypi/waybill-fixture-devlib@1.0 | shared: excluded waybill:lifecycle-scope=optional waybill:lifecycle-scope-derivation=transitive
r4-control run 1 | root: pkg:pypi/waybill-fixture-app@1.0.0 | edges into shared: pkg:pypi/waybill-fixture-app@1.0.0, pkg:pypi/waybill-fixture-devlib@1.0 | shared: required 
r4-control run 2 | root: pkg:pypi/waybill-fixture-app@1.0.0 | edges into shared: pkg:pypi/waybill-fixture-app@1.0.0, pkg:pypi/waybill-fixture-devlib@1.0 | shared: required 
r4-control run 3 | root: pkg:pypi/waybill-fixture-app@1.0.0 | edges into shared: pkg:pypi/waybill-fixture-app@1.0.0, pkg:pypi/waybill-fixture-devlib@1.0 | shared: required 
```
