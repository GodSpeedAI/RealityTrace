"""Real provider client for the v2 instrument (owner decision §3).

GLM-5.2 via the Z.ai GLM Coding Plan endpoint.

This is a NEW module. `eval/zai_client.py` — the client that executed the
historical experiments — is left byte-identical, so this cannot perturb the
historical record. Only its credential helpers are reused, and the credential
itself is never logged, returned, or persisted.

Everything the provider returns is recorded; nothing is invented. A field the
provider does not report is simply ABSENT from the returned `provider` dict, so
`execution.record_from_provider` marks it `null` with a reason rather than
coercing it to zero (the defect that made the historical usage sidecars
uninformative — audit NEW-03).

`finish_reason` is captured here specifically because the historical client
discarded it, which is why a 4096-token ceiling governed the entire chain
undetected (audit NEW-02).
"""

from __future__ import annotations

import json
import time
import urllib.error
import urllib.request

from ..zai_client import CODING_URL, MAX_RETRIES, RETRY_BACKOFF, RETRY_STATUS
from ..zai_client import _jwt, _read_key
from ..model_client import ModelClientError

PROVIDER_ROUTE = "zai-coding-plan"
ENDPOINT = CODING_URL


class ZaiCodingPlanClient:
    """Implements the `controller.ModelClient` protocol."""

    def __init__(self, model: str = "glm-5.2", timeout: float = 300.0):
        self.model = model
        self.timeout = timeout

    def complete(self, system: str, user: str, requested_max_tokens: int,
                 temperature: float) -> dict:
        body = json.dumps({
            "model": self.model,
            "temperature": temperature,
            "max_tokens": requested_max_tokens,
            "messages": [{"role": "system", "content": system},
                         {"role": "user", "content": user}],
        }).encode()
        headers = {"Content-Type": "application/json",
                   "Authorization": "Bearer " + _jwt(_read_key())}

        retries = 0
        data = None
        last_err = None
        elapsed = 0.0
        for attempt in range(MAX_RETRIES + 1):
            t0 = time.monotonic()
            req = urllib.request.Request(ENDPOINT, data=body, headers=headers)
            try:
                with urllib.request.urlopen(req, timeout=self.timeout) as r:
                    data = json.loads(r.read())
                elapsed = (time.monotonic() - t0) * 1000.0
                break
            except urllib.error.HTTPError as e:
                elapsed = (time.monotonic() - t0) * 1000.0
                if e.code == 401:
                    raise ModelClientError("z.ai coding-plan auth rejected (401)")
                if e.code in RETRY_STATUS and attempt < MAX_RETRIES:
                    retries += 1
                    time.sleep(RETRY_BACKOFF[attempt])
                    continue
                last_err = ModelClientError(f"HTTP {e.code}")
                break
            except (urllib.error.URLError, TimeoutError, OSError) as e:
                elapsed = (time.monotonic() - t0) * 1000.0
                if attempt < MAX_RETRIES:
                    retries += 1
                    time.sleep(RETRY_BACKOFF[attempt])
                    continue
                last_err = ModelClientError(f"transport error: {type(e).__name__}")
                break

        if data is None:
            raise ModelClientError(f"coding-plan call failed after "
                                   f"{retries} retries: {last_err}")

        choices = data.get("choices") or []
        if not choices:
            raise ModelClientError("coding-plan returned no choices")
        choice = choices[0]
        msg = choice.get("message") or {}
        usage = data.get("usage") or {}
        details = usage.get("completion_tokens_details") or {}

        provider = {"provider_route": PROVIDER_ROUTE, "retries": retries,
                    "latency_ms": round(elapsed, 1)}

        # Only record what was actually returned. Absent stays absent.
        for key, value in (
                ("resolved_model", data.get("model")),
                ("generation_id", data.get("id") or data.get("request_id")),
                ("finish_reason", choice.get("finish_reason")),
                ("prompt_tokens", usage.get("prompt_tokens")),
                ("completion_tokens", usage.get("completion_tokens")),
                ("total_tokens", usage.get("total_tokens")),
                ("reasoning_tokens", details.get("reasoning_tokens")),
                ("cost", usage.get("cost")),
        ):
            if value is not None:
                provider[key] = value

        return {"text": msg.get("content") or "", "provider": provider,
                "reasoning_present": bool(msg.get("reasoning_content"))}
