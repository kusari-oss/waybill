# Quickstart

    cargo nextest run -p waybill --test haskell_pin_annotations
    # #1741 fixture: unused freeze pin -> edgeless-lock-pin
    # Corpus (CI): gh workflow run public-corpus.yml -f branch=1666-haskell-pin-annotations -f regen_goldens=true
    specs/1666-haskell-pin-annotations/measurements/measure.sh <golden-dir> <label>
