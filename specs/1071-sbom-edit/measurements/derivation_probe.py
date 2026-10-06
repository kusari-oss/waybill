"""Milestone 1071 research R1: do the formats' validators accept the native
"this document is derived from that one" links, and do they reject wrong
values (negative controls)? Run from the repo root with the spdx3-validate
venv's python. Writes results to stdout."""
import copy, json, subprocess, sys, tempfile, pathlib
from jsonschema import Draft7Validator, Draft202012Validator
from jsonschema.validators import validator_for
from referencing import Registry, Resource

S = pathlib.Path("waybill-cli/tests/fixtures/schemas")
G = pathlib.Path("waybill-cli/tests/fixtures/golden")
H = "a" * 64
ORIG = "urn:sha256:" + H

def cdx_validator():
    root = json.loads((S / "cyclonedx-1.6.json").read_text())
    reg = Registry()
    for name in ["jsf-0.82.schema.json"]:
        res = Resource.from_contents(json.loads((S / name).read_text()))
        reg = reg.with_resource(name, res).with_resource("http://cyclonedx.org/schema/" + name, res)
    # spdx license list schema is referenced by id; allow any string
    spdx = Resource.from_contents({"$schema": "http://json-schema.org/draft-07/schema#", "type": "string"})
    reg = reg.with_resource("spdx.schema.json", spdx).with_resource("http://cyclonedx.org/schema/spdx.schema.json", spdx)
    cls = validator_for(root)
    return cls(root, registry=reg)

def errs(v, doc):
    return [e.message[:140] for e in v.iter_errors(doc)][:3]

def spdx3_validate(doc):
    with tempfile.NamedTemporaryFile("w", suffix=".json", delete=False) as f:
        json.dump(doc, f)
    r = subprocess.run([".venv/spdx3-validate/bin/spdx3-validate", "--json", f.name], capture_output=True, text=True)
    tail = [l for l in (r.stdout + r.stderr).splitlines() if "rror" in l or "not one of" in l or "Violation" in l][:3]
    return r.returncode, tail

out = []
# ---- CycloneDX 1.6
v = cdx_validator()
base = json.loads((G / "cyclonedx/cargo.cdx.json").read_text())
out.append(f"CDX golden baseline errors: {errs(v, base)}")
d = copy.deepcopy(base)
d.setdefault("externalReferences", []).append({"type": "bom", "url": ORIG, "comment": "derived from", "hashes": [{"alg": "SHA-256", "content": H}]})
out.append(f"CDX root externalReferences type=bom + SHA-256: errors={errs(v, d)}")
bad = copy.deepcopy(d); bad["externalReferences"][-1]["type"] = "derived-from"
out.append(f"CDX negative control type=derived-from: errors={errs(v, bad)}")

# ---- SPDX 2.3
s2 = json.loads((S / "spdx-2.3.json").read_text())
v2 = validator_for(s2)(s2)
base = json.loads((G / "spdx-2.3/cargo.spdx.json").read_text())
out.append(f"SPDX 2.3 golden baseline errors: {errs(v2, base)}")
d = copy.deepcopy(base)
d.setdefault("externalDocumentRefs", []).append({"externalDocumentId": "DocumentRef-original", "spdxDocument": ORIG, "checksum": {"algorithm": "SHA256", "checksumValue": H}})
d.setdefault("relationships", []).append({"spdxElementId": "SPDXRef-DOCUMENT", "relationshipType": "AMENDS", "relatedSpdxElement": "DocumentRef-original:SPDXRef-DOCUMENT"})
out.append(f"SPDX 2.3 externalDocumentRefs(SHA256) + AMENDS: errors={errs(v2, d)}")
bad = copy.deepcopy(d); bad["relationships"][-1]["relationshipType"] = "BUILT_FROM"
out.append(f"SPDX 2.3 control relationshipType=BUILT_FROM (m072 emits this): errors={errs(v2, bad)}")
bad = copy.deepcopy(d); bad["relationships"][-1]["relationshipType"] = "DERIVED_FROM"
out.append(f"SPDX 2.3 negative control DERIVED_FROM: errors={errs(v2, bad)}")

# ---- SPDX 3.0.1
base = json.loads((G / "spdx-3/cargo.spdx3.json").read_text())
rc, t = spdx3_validate(base); out.append(f"SPDX 3 golden baseline: rc={rc} {t}")
graph = base["@graph"]
docel = next(e for e in graph if e.get("type") == "SpdxDocument")
ci = docel.get("creationInfo")
def with_rel(rtype, frm, to):
    d = copy.deepcopy(base)
    de = next(e for e in d["@graph"] if e.get("type") == "SpdxDocument")
    de["import"] = de.get("import", []) + [{"type": "ExternalMap", "externalSpdxId": ORIG, "verifiedUsing": [{"type": "Hash", "algorithm": "sha256", "hashValue": H}]}]
    d["@graph"].append({"type": "Relationship", "spdxId": de["spdxId"] + "/relationship/probe", "creationInfo": ci, "from": frm(de), "to": [to(de)], "relationshipType": rtype})
    return d
cases = [
    ("amendedBy (original -> this)", "amendedBy", lambda de: ORIG, lambda de: de["spdxId"]),
    ("descendantOf (this -> original)", "descendantOf", lambda de: de["spdxId"], lambda de: ORIG),
    ("built_from (m072 emits this)", "built_from", lambda de: de["spdxId"], lambda de: ORIG),
    ("negative control derivedFrom", "derivedFrom", lambda de: de["spdxId"], lambda de: ORIG),
]
for label, rt, f, t in cases:
    rc, tail = spdx3_validate(with_rel(rt, f, t))
    out.append(f"SPDX 3 {label}: rc={rc} {tail}")
print("\n".join(out))
