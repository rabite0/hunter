#!/bin/sh

which 7z >/dev/null && EXTRACTOR="7z x"
# Prefer aunpack
which aunpack >/dev/null && EXTRACTOR=aunpack


for file in "$@"; do
    echo $EXTRACTOR "$file";
done
