#!/bin/bash
set -eu

cd "$SRC/sqlparser-canonicalize"
cargo fuzz build -O --debug-assertions --fuzz-dir fuzz

targets=$(cargo fuzz list --fuzz-dir fuzz)
if [ -z "$targets" ]; then
    echo "cargo fuzz list named no target" >&2
    exit 1
fi

target_dir=fuzz/target/x86_64-unknown-linux-gnu/release
for name in $targets; do
    cp "$target_dir/$name" "$OUT/"
    # every target reads SQL text, so the runner picks up <target>.dict for each
    cp fuzz/sql.dict "$OUT/$name.dict"
    # the input length guard sits at 8192 bytes, above libFuzzer's default max_len
    printf '[libfuzzer]\nmax_len = 10000\n' >"$OUT/$name.options"
done

# the runner unpacks <target>_seed_corpus.zip as the starting corpus
for dir in fuzz/seeds/*/; do
    name=$(basename "$dir")
    zip -qj "$OUT/${name}_seed_corpus.zip" "$dir"*
done
