#!/bin/sh

FILE="${1}"
BASENAME=`basename -s ".pdf" "${FILE}"`

mkdir /tmp/hunter-previews
pdftoppm -singlefile -singlefile "${FILE}" /tmp/hunter-previews/"${BASENAME}" || exit 1
echo /tmp/hunter-previews/"${BASENAME}".ppm
