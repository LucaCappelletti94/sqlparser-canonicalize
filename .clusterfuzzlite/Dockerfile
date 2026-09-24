FROM gcr.io/oss-fuzz-base/base-builder-rust
COPY . $SRC/sqlparser-canonicalize
WORKDIR $SRC/sqlparser-canonicalize
COPY ./.clusterfuzzlite/build.sh $SRC/
