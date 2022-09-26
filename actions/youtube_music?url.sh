#!/bin/sh

FORMATS=`youtube-dl -F "$url"`

echo $FORMATS

echo $FORMATS | grep "251 " &&
    youtube-dl -x -f 251 "$url" &&
    exit 0

echo $FORMATS | grep "171 " &&
    youtube-dl -x -f 171 "$url" &&
    exit 0

exit 1
