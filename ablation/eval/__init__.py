"""M2 evaluation harness — isolated retrieval-arm ablation driver.

Namespace layout (M2-J):
    runs/<namespace>/<experiment_id>/
        manifest.jsonl        # checkpoint store (unit states)
        matrix.json           # seeded matrix + recorded order
        order.json            # randomized execution order + seed
        raw/<unit_id>.txt     # committed raw model outputs
        bundles/<unit_id>/    # sanitized eval bundles
        scoring/<unit_id>.json  # deterministic scoring records
"""

__version__ = "0.2.0-m2"
