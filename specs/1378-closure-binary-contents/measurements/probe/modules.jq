# Every Go or cargo PURL anywhere in a document, whatever the format.
[.. | strings | select(test("^pkg:(golang|cargo)/"))] | unique
