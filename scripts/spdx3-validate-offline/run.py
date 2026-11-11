"""Run `spdx3-validate` 0.0.5 without network access (#1417).

The validator fetches three SPDX 3.0.1 resources on every invocation:

  * the JSON-LD context named by the document's `@context`, which rdflib
    resolves while parsing the document as JSON-LD;
  * the JSON schema, read with `urllib.request.urlopen`;
  * the SHACL model, read with `rdflib.Graph.parse(url)`.

It has no option to point at local copies, so this wrapper installs a
process-wide `urllib` opener (`urllib.request.install_opener`, the stdlib's
own hook, which both urllib and rdflib go through) that serves those three
URLs from the copies vendored next to this file, then calls the installed
validator's `main()` unchanged. The installed package is not modified.

Any other http(s) URL is refused rather than fetched, so a validator
upgrade that starts needing a fourth resource fails loudly here instead of
quietly putting the network back into the test suite.

Run it with the validator venv's interpreter, isolated:

    .venv/spdx3-validate/bin/python -I scripts/spdx3-validate-offline/run.py --quiet -j doc.json

Vendored resources were fetched on 2026-11-10. spdx.org redirects the
context and model URLs to spdx.github.io; the content below is what the
redirect served. They are published by the SPDX project under the
Community Specification License 1.0 (the model's `dcterms:license`).
Refreshing them, or adding a 3.0.0 set, is a deliberate edit: replace the
files and update the digests below.
"""

import email.message
import hashlib
import io
import sys
import urllib.request
import urllib.response
from pathlib import Path

RESOURCES = Path(__file__).resolve().parent / "resources"

# URL the validator requests -> (vendored path, Content-Type, SHA-256).
VENDORED = {
    "https://spdx.org/rdf/3.0.1/spdx-context.jsonld": (
        "3.0.1/spdx-context.jsonld",
        "application/ld+json",
        "c72b0928f094c83e5c127784edb1ebca2af74a104fcacc007c332b23cbc788bd",
    ),
    "https://spdx.org/rdf/3.0.1/spdx-model.ttl": (
        "3.0.1/spdx-model.ttl",
        "text/turtle",
        "30ebb4af2d70a9809044ef46f44cc3dc5125226d70f818a50ed2e1d5f404c593",
    ),
    "https://spdx.org/schema/3.0.1/spdx-json-schema.json": (
        "3.0.1/spdx-json-schema.json",
        "application/json",
        "582c64e809d5b3ef9bd0c4de13a32391b47b0284a3e8d199569fb96f649234b1",
    ),
}

# Distinct from the validator's own exit codes (0 pass, 1 violations).
EXIT_OFFLINE = 3


class NotVendored(Exception):
    """The validator asked for a URL that has no vendored copy."""


def load_vendored():
    contents = {}
    for url, (rel, content_type, sha256) in VENDORED.items():
        data = (RESOURCES / rel).read_bytes()
        actual = hashlib.sha256(data).hexdigest()
        if actual != sha256:
            sys.exit(
                f"spdx3-validate-offline: {RESOURCES / rel} has SHA-256 {actual}, "
                f"expected {sha256}. The vendored copy of {url} was modified."
            )
        contents[url] = (data, content_type)
    return contents


class VendoredHandler(urllib.request.BaseHandler):
    # Ahead of ProxyHandler (100), so no proxy or socket is ever consulted.
    handler_order = 0

    def __init__(self, contents):
        self.contents = contents

    def serve(self, req):
        url = req.full_url
        if url not in self.contents:
            raise NotVendored(url)
        data, content_type = self.contents[url]
        headers = email.message.Message()
        headers["Content-Type"] = content_type
        headers["Content-Length"] = str(len(data))
        response = urllib.response.addinfourl(io.BytesIO(data), headers, url, 200)
        # Read by HTTPErrorProcessor, which runs on every http(s) response.
        response.msg = "OK"
        return response

    http_open = serve
    https_open = serve


def main():
    urllib.request.install_opener(
        urllib.request.build_opener(VendoredHandler(load_vendored()))
    )
    from spdx3_validate.main import main as validate

    sys.argv[0] = "spdx3-validate"
    try:
        return validate()
    except NotVendored as e:
        # Deliberately worded without "URLError"/"urlopen error": callers
        # classify those as a transient network failure and retry.
        print(
            f"spdx3-validate-offline: the validator requested {e}, which has no "
            "vendored copy under scripts/spdx3-validate-offline/resources/. "
            "Vendor it (see run.py) rather than letting tests reach the network.",
            file=sys.stderr,
        )
        return EXIT_OFFLINE


if __name__ == "__main__":
    sys.exit(main())
