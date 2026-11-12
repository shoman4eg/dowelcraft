#!/bin/sh
# Rebuilds every file in exports/ from kitchen.dcraft. Run from this folder;
# DOWEL points at the program (default: the release build of this repository).
set -e
DOWEL=${DOWEL:-../../target/release/dowel-craft}
mkdir -p exports
"$DOWEL" spec     kitchen.dcraft -o exports/parts.csv
"$DOWEL" smeta    kitchen.dcraft -o exports/estimate.csv
"$DOWEL" cut      kitchen.dcraft -o exports/cutting.html
"$DOWEL" cut      kitchen.dcraft -o exports/cutting.pdf
"$DOWEL" raskroy  kitchen.dcraft -o exports/bazis-raskroy.xml
"$DOWEL" drawings kitchen.dcraft -o exports/drawings.html
"$DOWEL" drawings kitchen.dcraft -o exports/drawings.pdf
"$DOWEL" obj      kitchen.dcraft -o exports/kitchen.obj
"$DOWEL" b3d      kitchen.dcraft -o exports/kitchen.b3d
