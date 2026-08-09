"""M3 context-budget packer — enforces the frozen 32K total-input contract.

The frozen experiment declares `context_budget_tokens = 32_000`. That is
interpreted as MAXIMUM TOTAL MODEL INPUT TOKENS (conservative interpretation
A): system prompt + task scaffold + case line + tool/control note + omission
statement + retrieved context together must not exceed 32,000 tokens.

The harness MUST enforce this contract. This module packs each arm's retrieved
context deterministically and answer-blind into the retrieval budget, so every
live prompt obeys the frozen limit. The budget rule is arm-independent: all
four arms receive the same maximum input budget.

Budget model
    total_budget = context_budget_tokens (32_000)
    fixed = system_prompt + task_scaffold_frame + case_line
            + tool/control_note + omission_statement (estimated)
    safety_margin = 12% of total_budget (fixed harness rule, not tuned to a case)
    retrieval_budget = total_budget - fixed_tokens - safety_margin

Packing (deterministic, answer-blind)
    Retrieved items (capability sections present in the arm) are ordered by a
    predeclared ranking (sql=0, fts=1, vector=2, graph=3) and added whole while
    retrieval_budget remains. If an item exceeds the remaining budget it is:
      - excerpted deterministically IF its representation type has a
        predeclared excerpt rule (FTS: leading whole lines up to the item
        budget), OR
      - otherwise omitted.
    Every included/excerpted/omitted item and its token counts + provenance are
    recorded. If anything was truncated, a neutral omission statement is added
    to the prompt (no LLM summarization of omitted material).

Token counting
    No provider tokenizer is locally available (tiktoken/transformers absent).
    A single deterministic byte-based estimator is used with a conservative
    ratio (2.5 bytes/token, over-estimates tokens vs the measured ~3.05) plus
    the fixed 12% safety margin. Limitation recorded. Compliance is VERIFIED
    from the provider-returned `usage.prompt_tokens` after every live call: any
    prompt > 32_000 is a protocol failure.
"""

from __future__ import annotations

import hashlib
import json
import os
from dataclasses import dataclass, field

from .contract import TASK_PROMPT_SCAFFOLD

PACKING_VERSION = "pack-v1"
BYTES_PER_TOKEN = 2.5          # conservative over-estimate (measured ~3.05)
SAFETY_MARGIN_FRACTION = 0.12  # fixed harness-level rule

# Predeclared ranking (arm-independent; each arm uses its own subset).
CAP_RANK = {"sql": 0, "fts": 1, "vector": 2, "graph": 3}

# Representation types that have a predeclared excerpt rule.
EXCERPT_CAPABILITIES = ("fts",)

OMISSION_STATEMENT = (
    "Additional retrieved material was omitted due to the fixed experiment "
    "context budget."
)

TOOL_CONTROL_NOTE = (
    "Tool/control: read-only failure diagnosis; no recovery or repair actions."
)


class BudgetError(Exception):
    pass


@dataclass
class PackedItem:
    capability: str
    status: str                 # included | excerpted | omitted
    item_tokens: int
    included_tokens: int
    omitted_tokens: int
    excerpt_rule: str | None = None

    def to_dict(self) -> dict:
        return {
            "capability": self.capability, "status": self.status,
            "item_tokens": self.item_tokens,
            "included_tokens": self.included_tokens,
            "omitted_tokens": self.omitted_tokens,
            "excerpt_rule": self.excerpt_rule,
        }


@dataclass
class PackedContext:
    prompt: str                 # the full user prompt (case + packed context)
    sections: dict[str, str]    # included section text by capability
    items: list[PackedItem] = field(default_factory=list)
    fixed_tokens: int = 0
    retrieval_budget: int = 0
    total_budget: int = 0
    safety_margin_tokens: int = 0
    estimated_prompt_tokens: int = 0
    packing_version: str = PACKING_VERSION
    omitted_material: bool = False

    def to_dict(self) -> dict:
        return {
            "packing_version": self.packing_version,
            "total_budget": self.total_budget,
            "fixed_tokens": self.fixed_tokens,
            "safety_margin_tokens": self.safety_margin_tokens,
            "retrieval_budget": self.retrieval_budget,
            "estimated_prompt_tokens": self.estimated_prompt_tokens,
            "omitted_material": self.omitted_material,
            "items": [i.to_dict() for i in sorted(
                self.items, key=lambda x: CAP_RANK.get(x.capability, 99))],
        }


def estimate_tokens(text: str) -> int:
    """Deterministic conservative token estimate (bytes / ratio, ceil).

    Conservative: uses 2.5 bytes/token (over-estimates tokens relative to the
    measured ~3.05 bytes/token), so packed prompts stay safely under budget.
    """
    if not text:
        return 0
    return max(1, -(-len(text.encode("utf-8")) * 10 // int(
        BYTES_PER_TOKEN * 10)))


def _est(text: str) -> int:
    """Alias used internally; identical to estimate_tokens."""
    return estimate_tokens(text)


def pack_context(sections: dict[str, str], system_prompt: str, case_id: str,
                 arm: str, budget: int = 32_000) -> PackedContext:
    """Deterministically pack retrieved sections into the frozen total input
    budget. Returns the assembled prompt + a full packing record."""
    # ---- fixed reservations (interpretation A: total model input) ----
    prefix, _, suffix = TASK_PROMPT_SCAFFOLD.partition("{context}")
    scaffold_frame = prefix + suffix
    case_line = "Diagnose the recorded failure for case %s." % case_id
    fixed_text = "\n".join([system_prompt, scaffold_frame, case_line,
                            TOOL_CONTROL_NOTE, OMISSION_STATEMENT])
    fixed_tokens = _est(fixed_text)
    safety_margin_tokens = int(budget * SAFETY_MARGIN_FRACTION)
    retrieval_budget = budget - fixed_tokens - safety_margin_tokens
    if retrieval_budget <= 0:
        raise BudgetError(
            "frozen budget too small to hold mandatory prompt material; "
            "return to owner review")

    # ---- pack retrieved items in predeclared ranking order ----
    items: list[PackedItem] = []
    included: dict[str, str] = {}
    remaining = retrieval_budget
    omitted_material = False
    for cap in sorted(sections.keys(), key=lambda c: CAP_RANK.get(c, 99)):
        text = sections[cap]
        item_tokens = _est(text)
        if item_tokens <= remaining:
            included[cap] = text
            remaining -= item_tokens
            items.append(PackedItem(cap, "included", item_tokens,
                                    item_tokens, 0))
            continue
        if cap in EXCERPT_CAPABILITIES:
            # deterministic bounded excerpt: leading whole lines within budget
            excerpt, excerpt_tokens, omitted_tokens = _excerpt_prefix(
                text, remaining)
            if excerpt:
                included[cap] = excerpt
                remaining -= excerpt_tokens
                omitted_material = True
                items.append(PackedItem(cap, "excerpted", item_tokens,
                                        excerpt_tokens, omitted_tokens,
                                        excerpt_rule="fts-leading-lines-v1"))
                continue
        # otherwise omit
        items.append(PackedItem(cap, "omitted", item_tokens, 0, item_tokens))
        omitted_material = omitted_material or item_tokens > 0

    # ---- assemble the prompt ----
    if not included:
        raise BudgetError("no retrieved material could be packed into budget")
    context_text = "\n\n".join(
        "[%s]\n%s" % (c, included[c]) for c in sorted(
            included.keys(), key=lambda c: CAP_RANK.get(c, 99)))
    if omitted_material:
        context_text += "\n\n" + OMISSION_STATEMENT
    prompt = TASK_PROMPT_SCAFFOLD.format(case_id=case_id, arm=arm,
                                         context=context_text)
    estimated_prompt_tokens = _est(
        "\n".join([system_prompt, prompt, TOOL_CONTROL_NOTE]))

    return PackedContext(
        prompt=prompt, sections=included, items=items,
        fixed_tokens=fixed_tokens, retrieval_budget=retrieval_budget,
        total_budget=budget, safety_margin_tokens=safety_margin_tokens,
        estimated_prompt_tokens=estimated_prompt_tokens,
        packing_version=PACKING_VERSION, omitted_material=omitted_material)


def _excerpt_prefix(text: str, token_budget: int) -> tuple[str, int, int]:
    """Deterministic bounded excerpt: leading whole lines fitting the budget.

    Returns (excerpt, included_tokens, omitted_tokens). Whole-line boundary
    (not arbitrary byte slicing); deterministic and answer-blind.
    """
    if token_budget <= 0:
        return "", 0, len(text.encode("utf-8"))
    lines = text.splitlines(keepends=True)
    included: list[str] = []
    used = 0
    for ln in lines:
        t = _est(ln)
        if used + t > token_budget:
            break
        included.append(ln)
        used += t
    excerpt = "".join(included)
    omitted = len(text.encode("utf-8")) - len(excerpt.encode("utf-8"))
    return excerpt, used, max(0, omitted)