"""Loopback OpenCode session/message transport for Evidence Revision 002 exec004.

This adapter never contacts a model provider directly.  It talks only to an
already authenticated, locally-operated ``opencode serve`` process on loopback.
Every completion creates one new restricted session, sends exactly one user text
part, and never reuses that session.  RealityTrace retains all scientific state,
controller, parsing, scoring and durability authority.
"""
from __future__ import annotations

import json
import subprocess
import time
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path
from typing import Callable

PROVIDER_ROUTE = "opencode-runtime"
PROVIDER_ID = "opencode"
MODEL = "deepseek-v4-flash-free"
ENDPOINT = "http://127.0.0.1:4096"
SESSION_AGENT = "er002-transport"
# Tool names understood by the contemporary OpenCode service.  Explicit false
# plus the deny-all per-session permission ruleset provides defense in depth.
DENIED_TOOLS = {name: False for name in (
    "bash", "shell", "read", "write", "edit", "apply_patch", "glob", "grep",
    "list", "webfetch", "websearch", "browser", "task", "subtask", "agent",
    "skill", "mcp", "question", "lsp", "todo", "codesearch")}
DENY_ALL_PERMISSIONS = [{"permission": "*", "pattern": "*", "action": "deny"}]
# This exact configuration fragment must be loaded by the loopback server for
# qualification/FULL. Passing the named agent per request prevents fallback to
# OpenCode's default build agent; both its tools and permissions deny all.
RESTRICTED_AGENT_CONFIG = {"agent": {SESSION_AGENT: {
    "description": "RealityTrace completion transport; tools forbidden.",
    "tools": DENIED_TOOLS,
    "permission": {"*": "deny"},
}}}
RETRYABLE_HTTP = frozenset((429, 500, 502, 503, 504))

class ProviderFailure(RuntimeError):
    def __init__(self, classification: str, reason: str, status: int | None = None):
        super().__init__(reason); self.classification, self.status = classification, status

class AmbiguousTransport(RuntimeError):
    """Request fate is unknown.  The containing shard must be frozen."""

def _safe_reason(exc: BaseException) -> str:
    # Never preserve headers or arbitrary response bodies; neither may be safe.
    errno = getattr(exc, "errno", None)
    return type(exc).__name__ + (f" errno={errno}" if errno is not None else "")

def _loopback(url: str) -> bool:
    parsed=urllib.parse.urlparse(url)
    return parsed.scheme == "http" and parsed.hostname in {"127.0.0.1", "localhost", "::1"} and not parsed.username and not parsed.password

def installed_runtime_version(binary: str = "opencode") -> str | None:
    """Read local runtime identity only; never opens a server or sends a prompt."""
    return _runtime_version(binary)

def _runtime_version(binary: str) -> str | None:
    try:
        value=subprocess.check_output([binary,"--version"],stderr=subprocess.DEVNULL,text=True,timeout=10).strip()
    except (OSError, subprocess.SubprocessError):
        return None
    return value or None

class OpenCodeRuntimeClient:
    """Documented ``POST /session`` + ``POST /session/{id}/message`` client."""
    provider_route=PROVIDER_ROUTE
    endpoint=ENDPOINT
    model=MODEL
    offline_only=False

    def __init__(self, endpoint: str = ENDPOINT, timeout: float = 300.0,
                 workspace_directory: Path | str = "/tmp/sxr-opencode-transport",
                 opencode_binary: str = "opencode", attempt_sink: Callable[[dict],None] | None = None,
                 opener: Callable = urllib.request.urlopen, sleep: Callable = time.sleep):
        # Assignment only.  In particular, no credential lookup, subprocess or
        # socket I/O happens at construction.
        if not _loopback(endpoint):
            raise ValueError("OpenCode runtime endpoint must be unauthenticated loopback HTTP")
        self.endpoint=endpoint.rstrip("/"); self.timeout=timeout
        self.workspace_directory=Path(workspace_directory).resolve()
        self.opencode_binary=opencode_binary; self._sink=attempt_sink; self._opener=opener; self._sleep=sleep
        self.runtime_version: str | None=None

    def set_attempt_sink(self, sink: Callable[[dict],None] | None) -> None: self._sink=sink
    def _record(self,row:dict) -> None:
        if self._sink: self._sink(dict(row))

    def validate_local_configuration(self) -> dict:
        # This is explicitly local-only.  It validates the binary and fixed
        # isolation policy but does not probe the HTTP server or contact a model.
        self.runtime_version=_runtime_version(self.opencode_binary)
        if self.runtime_version is None: raise ProviderFailure("REJECTED_FATAL","opencode runtime binary unavailable")
        if self.workspace_directory == Path("/") or self.workspace_directory == Path.cwd():
            raise ProviderFailure("REJECTED_FATAL","OpenCode workspace must be an isolated non-repository directory")
        return {"configured":True,"model":self.model,"provider_route":self.provider_route,"endpoint":self.endpoint,
                "provider_id":PROVIDER_ID,"runtime_version":self.runtime_version,"agent":SESSION_AGENT,
                "tools":dict(DENIED_TOOLS),"permission":list(DENY_ALL_PERMISSIONS),
                "required_runtime_config":RESTRICTED_AGENT_CONFIG,"network_requests":0}

    def _url(self,path:str) -> str:
        query=urllib.parse.urlencode({"directory":str(self.workspace_directory)})
        return self.endpoint+path+"?"+query

    def _http(self, operation:str,path:str, body:dict, number:int) -> tuple[dict,int]:
        self._record({"attempt_num":number,"operation":operation,"status":"INTENT","classification":None,
                      "endpoint":self.endpoint+path})
        request=urllib.request.Request(self._url(path),data=json.dumps(body,separators=(",",":"),sort_keys=True).encode(),
          headers={"Content-Type":"application/json"},method="POST")
        try:
            with self._opener(request,timeout=self.timeout) as response:
                status=getattr(response,"status",None) or response.getcode(); payload=json.loads(response.read())
        except urllib.error.HTTPError as exc:
            classification="REJECTED_RETRYABLE" if exc.code in RETRYABLE_HTTP else "REJECTED_FATAL"
            self._record({"attempt_num":number,"operation":operation,"status":"REJECTED","classification":classification,"http_status":exc.code,"reason":f"HTTP {exc.code}"})
            raise ProviderFailure(classification,f"HTTP {exc.code}",exc.code) from exc
        except (urllib.error.URLError,TimeoutError,ConnectionResetError,OSError,json.JSONDecodeError) as exc:
            reason=_safe_reason(exc); self._record({"attempt_num":number,"operation":operation,"status":"AMBIGUOUS","classification":"AMBIGUOUS_TRANSPORT","reason":reason})
            raise AmbiguousTransport(reason) from exc
        if not isinstance(payload,dict):
            self._record({"attempt_num":number,"operation":operation,"status":"REJECTED","classification":"REJECTED_FATAL","http_status":status,"reason":"invalid OpenCode response"})
            raise ProviderFailure("REJECTED_FATAL","invalid OpenCode response",status)
        self._record({"attempt_num":number,"operation":operation,"status":"DELIVERED","classification":"DELIVERED","http_status":status})
        return payload,status

    def _one_completion(self, system:str,user:str, attempt_base:int = 0) -> dict:
        session,status=self._http("create_session","/session",{
          "title":"realitytrace-exec004-transport","agent":SESSION_AGENT,
          "model":{"id":self.model,"providerID":PROVIDER_ID},
          "metadata":{"realitytrace":"exec004","isolation":"one-session-per-call","tools":"deny-all"},
          "permission":DENY_ALL_PERMISSIONS},attempt_base + 1)
        session_id=session.get("id")
        if not isinstance(session_id,str) or not session_id:
            raise ProviderFailure("REJECTED_FATAL","OpenCode did not return a session id",status)
        message,status=self._http("send_message",f"/session/{urllib.parse.quote(session_id,safe='')}/message",{
          "model":{"providerID":PROVIDER_ID,"modelID":self.model},"agent":SESSION_AGENT,
          "tools":DENIED_TOOLS,"system":system,"parts":[{"type":"text","text":user}]},attempt_base + 2)
        parts=message.get("parts"); info=message.get("info") or {}
        if not isinstance(parts,list): raise ProviderFailure("REJECTED_FATAL","OpenCode response lacks message parts",status)
        text="".join(p.get("text","") for p in parts if isinstance(p,dict) and p.get("type")=="text" and isinstance(p.get("text"),str))
        if not text: raise ProviderFailure("REJECTED_FATAL","OpenCode response lacks assistant text",status)
        model=info.get("model") if isinstance(info,dict) else None
        provider={"provider_route":PROVIDER_ROUTE,"opencode_version":self.runtime_version,"provider_id":PROVIDER_ID,
          "requested_model":self.model,"session_id":session_id,"message_id":info.get("id") if isinstance(info,dict) else None,
          "latency_ms":None,"retries":0,"tools_disabled":True,"session_isolated":True}
        if isinstance(model,dict):
            if model.get("modelID") is not None: provider["resolved_model"]=model["modelID"]
            if model.get("providerID") is not None: provider["resolved_provider_id"]=model["providerID"]
        return {"text":text,"provider":{k:v for k,v in provider.items() if v is not None}}

    def complete(self,system:str,user:str,requested_max_tokens:int,temperature:float) -> dict:
        # OpenCode's documented session/message API exposes model and messages,
        # but not controller temperature/max-token fields.  They are retained by
        # RealityTrace as requested provenance and are never invented here.
        if self.runtime_version is None: self.runtime_version=_runtime_version(self.opencode_binary)
        retries=0
        while True:
            started=time.monotonic()
            try:
                response=self._one_completion(system,user,retries * 2)
                response["provider"]["latency_ms"]=round((time.monotonic()-started)*1000,3)
                response["provider"]["retries"]=retries
                return response
            except ProviderFailure as exc:
                if exc.classification=="REJECTED_RETRYABLE" and retries < 2:
                    retries += 1; self._sleep(0); continue
                raise
