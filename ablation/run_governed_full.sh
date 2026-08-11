#!/bin/bash
cd /home/sprime01/projects/sxr/ablation
export SXR_ABLATION_RELEASE=acq-token-20260810
export SXR_ABLATION_RELEASE_MARKER=/home/sprime01/projects/sxr/ablation/RELEASE.gate
export PYTHONUNBUFFERED=1
exec uv run --python 3.12 python3 -m eval.governed_run
