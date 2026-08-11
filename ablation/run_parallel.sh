#!/bin/bash
cd /home/sprime01/projects/sxr/ablation
export SXR_ABLATION_RELEASE=acq-token-20260810
export SXR_ABLATION_RELEASE_MARKER=/home/sprime01/projects/sxr/ablation/RELEASE.gate
export PYTHONUNBUFFERED=1

WORKERS=3
for i in $(seq 0 $((WORKERS-1))); do
  uv run --python 3.12 python3 -m eval.governed_run --worker $i --num-workers $WORKERS > /tmp/gov-worker-$i.log 2>&1 &
  echo "Worker $i PID $!"
  sleep 5
done
wait
echo "ALL WORKERS DONE"
