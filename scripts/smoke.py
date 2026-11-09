#!/usr/bin/env python3
"""End-to-end smoke test for datajud-mcp over stdio.

Speaks newline-delimited JSON-RPC 2.0 (MCP stdio transport) to the server
binary: initialize -> tools/list -> resources/list -> prompts/list ->
selected tools/call (offline + live against the public API).

Usage:
    python3 scripts/smoke.py            # offline assertions only
    python3 scripts/smoke.py --live     # also calls the real DATAJUD API
"""

import json
import subprocess
import sys
import os
import time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
LIVE = "--live" in sys.argv


def find_binary():
    """Locates the server binary: DATAJUD_MCP_BIN override, else debug, else release."""
    override = os.environ.get("DATAJUD_MCP_BIN")
    if override:
        return override
    candidates = [
        os.path.join(ROOT, "target", "debug", "datajud-mcp"),
        os.path.join(ROOT, "target", "release", "datajud-mcp"),
    ]
    for candidate in candidates:
        if os.path.exists(candidate):
            return candidate
    sys.exit(
        "ERROR: datajud-mcp binary not found. Run `cargo build` (or `cargo build "
        "--release`) first, or point DATAJUD_MCP_BIN at the binary."
    )


BIN = find_binary()

ok = 0
fail = 0


def check(name, cond, detail=""):
    global ok, fail
    if cond:
        ok += 1
        print(f"  PASS  {name}")
    else:
        fail += 1
        print(f"  FAIL  {name}  {detail}")


class Session:
    def __init__(self, binary):
        env = dict(os.environ)
        # The server requires DATAJUD_API_KEY; offline checks never call the
        # API, so a placeholder is fine there. --live needs the real key.
        env.setdefault("DATAJUD_API_KEY", "offline-smoke-placeholder")
        self.proc = subprocess.Popen(
            [binary],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            text=True,
            bufsize=1,
            env=env,
        )
        self.next_id = 1

    def request(self, method, params=None):
        rid = self.next_id
        self.next_id += 1
        msg = {"jsonrpc": "2.0", "id": rid, "method": method}
        if params is not None:
            msg["params"] = params
        self.proc.stdin.write(json.dumps(msg) + "\n")
        self.proc.stdin.flush()
        while True:
            line = self.proc.stdout.readline()
            if not line:
                raise RuntimeError(f"server closed stream during {method}")
            data = json.loads(line)
            if data.get("id") == rid:
                if "error" in data:
                    raise RuntimeError(f"{method} -> {data['error']}")
                return data.get("result", {})

    def notify(self, method, params=None):
        msg = {"jsonrpc": "2.0", "method": method}
        if params is not None:
            msg["params"] = params
        self.proc.stdin.write(json.dumps(msg) + "\n")
        self.proc.stdin.flush()

    def call_tool(self, name, arguments):
        return self.request("tools/call", {"name": name, "arguments": arguments})

    def close(self):
        try:
            self.proc.stdin.close()
            self.proc.wait(timeout=5)
        except Exception:
            self.proc.kill()


def text_of(result):
    parts = []
    for block in result.get("content", []):
        if block.get("type") == "text":
            parts.append(block["text"])
    return "\n".join(parts)


def call_tool_retry(s, name, arguments, expect, attempts=3, delay=20):
    """Live calls can be flaky on the overloaded public cluster — retry."""
    last = None
    for i in range(attempts):
        r = s.call_tool(name, arguments)
        try:
            data = json.loads(text_of(r))
        except Exception:
            data = None
        last = (r, data)
        if data is not None and expect(data):
            return r, data
        print(f"       retry {name} (attempt {i + 1}/{attempts})...")
        time.sleep(delay)
    return last


def main():
    if LIVE and not os.environ.get("DATAJUD_API_KEY"):
        print("ERROR: --live requires DATAJUD_API_KEY in the environment")
        sys.exit(2)
    print(f"== datajud-mcp smoke test (live={LIVE}) ==")
    s = Session(BIN)

    # 1. initialize
    init = s.request(
        "initialize",
        {
            "protocolVersion": "2025-11-25",
            "capabilities": {},
            "clientInfo": {"name": "smoke", "version": "0.0.1"},
        },
    )
    check("initialize", init.get("serverInfo", {}).get("name") == "datajud-mcp", init)
    check(
        "capabilities declared",
        all(k in init.get("capabilities", {}) for k in ("tools", "resources", "prompts")),
        init.get("capabilities"),
    )
    s.notify("notifications/initialized")

    # 2. tools/list
    tools = s.request("tools/list")["tools"]
    names = sorted(t["name"] for t in tools)
    expected = sorted(
        [
            "aggregate_cases",
            "count_cases",
            "get_case",
            "list_fields",
            "list_tribunals",
            "search_cases",
            "search_dsl",
            "test_connection",
        ]
    )
    check("tools/list has all 8 tools", names == expected, names)
    for t in tools:
        check(
            f"tool '{t['name']}' has description + inputSchema",
            bool(t.get("description")) and bool(t.get("inputSchema")),
        )

    # 3. resources/list + templates
    resources = s.request("resources/list")["resources"]
    uris = sorted(r["uri"] for r in resources)
    check(
        "resources/list has 4 docs",
        uris
        == sorted(
            [
                "datajud://examples",
                "datajud://glossary",
                "datajud://terms-of-use",
                "datajud://tribunals",
            ]
        ),
        uris,
    )
    templates = s.request("resources/templates/list")["resourceTemplates"]
    check(
        "resource template datajud://tribunals/{alias}",
        any(t["uriTemplate"] == "datajud://tribunals/{alias}" for t in templates),
        templates,
    )

    # 4. resources/read
    gloss = s.request("resources/read", {"uri": "datajud://glossary"})
    body = gloss["contents"][0]["text"]
    check("glossary resource readable", "numeroProcesso" in body and "movimentos" in body)
    trib = s.request("resources/read", {"uri": "datajud://tribunals"})
    check("tribunals resource readable", trib["contents"][0]["text"].count("`tj") > 20)
    one = s.request("resources/read", {"uri": "datajud://tribunals/tjsp"})
    check(
        "tribunal template resource",
        "api_publica_tjsp" in one["contents"][0]["text"],
        one["contents"][0]["text"][:120],
    )
    missing = None
    try:
        s.request("resources/read", {"uri": "datajud://tribunals/nope"})
    except RuntimeError as e:
        missing = str(e)
    check("unknown tribunal alias -> error", missing is not None, missing)

    # 5. prompts/list + prompts/get
    prompts = s.request("prompts/list")["prompts"]
    pnames = sorted(p["name"] for p in prompts)
    check(
        "prompts/list has 3 prompts",
        pnames == ["analyze_case", "build_query", "research_report"],
        pnames,
    )
    gp = s.request(
        "prompts/get",
        {"name": "build_query", "arguments": {"goal": "automated testing"}},
    )
    check(
        "prompts/get returns messages",
        "automated testing" in gp["messages"][0]["content"]["text"],
    )

    # 6. offline tool calls
    r = s.call_tool("list_tribunals", {})
    data = json.loads(text_of(r))
    check("list_tribunals -> 91", data["total"] == 91 and data["available"] == 91, data.get("total"))
    r = s.call_tool("list_tribunals", {"segment": "military"})
    data = json.loads(text_of(r))
    check("list_tribunals filter segment", data["total"] == 3, data.get("total"))
    r = s.call_tool("list_tribunals", {"segment": "wrong"})
    check("invalid segment -> tool error", r.get("isError") is True, r)
    r = s.call_tool("list_fields", {})
    data = json.loads(text_of(r))
    check("list_fields -> >=28 fields", data["total"] >= 28, data.get("total"))
    r = s.call_tool("search_cases", {"tribunal": "does-not-exist"})
    check("unknown tribunal -> tool error", r.get("isError") is True, text_of(r)[:100])
    r = s.call_tool("search_dsl", {"tribunal": "tjsp", "body": "not-an-object"})
    check("search_dsl rejects non-object body", r.get("isError") is True, r)

    if LIVE:
        print("-- live API calls (public DATAJUD endpoint) --")
        r = s.call_tool("test_connection", {"tribunal": "tjms"})
        data = json.loads(text_of(r))
        check("test_connection ok", data.get("status") == "ok", text_of(r)[:200])
        print(f"       latency {data.get('latency_ms')} ms, total {data.get('total_documents')}")

        # wiki example 1: known case on TRF1
        r, data = call_tool_retry(
            s,
            "get_case",
            {"tribunal": "trf1", "process_number": "00008323520184013202"},
            expect=lambda d: d.get("found") is True,
        )
        check(
            "get_case wiki example (TRF1 00008323520184013202)",
            data.get("found") is True
            and data["case"]["data"]["numeroProcesso"] == "00008323520184013202",
            text_of(r)[:200],
        )
        if data and data.get("found"):
            check(
                "get_case includes movimentos",
                len(data["case"]["data"].get("movimentos", [])) > 0,
            )

        # structured search (wiki example 2 style: class filter on TJDFT)
        r, data = call_tool_retry(
            s,
            "search_cases",
            {
                "tribunal": "tjdft",
                "filters": {"class_code": 1116},
                "size": 2,
                "include_movements": False,
            },
            expect=lambda d: isinstance(d, dict) and d.get("total", {}).get("value", 0) > 0,
        )
        check(
            "search_cases class filter (TJDFT classe.codigo=1116)",
            data.get("total", {}).get("value", 0) > 0 and len(data.get("cases", [])) == 2,
            text_of(r)[:200],
        )
        if data and data.get("cases"):
            check(
                "search_cases trims movimentos by default",
                "movimentos" not in data["cases"][0]["data"],
            )
        check("search_cases paginates", data and "next_page" in data, list((data or {}).keys()))

        # count
        r, data = call_tool_retry(
            s,
            "count_cases",
            {"tribunal": "tjms", "filters": {"filing_date_from": "2024-01-01", "filing_date_until": "2024-01-02"}},
            expect=lambda d: isinstance(d, dict) and "by_tribunal" in d,
        )
        check(
            "count_cases live",
            data and data.get("total", -1) >= 0 and len(data.get("by_tribunal", [])) == 1,
            text_of(r)[:150],
        )

        # aggregation
        r, data = call_tool_retry(
            s,
            "aggregate_cases",
            {
                "tribunal": "tjms",
                "kind": "terms",
                "field": "classe.codigo",
                "bucket_size": 5,
                "filters": {"filing_date_from": "2024-01-01", "filing_date_until": "2024-01-02"},
            },
            expect=lambda d: isinstance(d, dict) and isinstance(d.get("buckets"), list) and len(d["buckets"]) > 0,
        )
        check(
            "aggregate_cases terms buckets",
            data and isinstance(data.get("buckets"), list) and len(data["buckets"]) > 0,
            text_of(r)[:200],
        )

        # raw DSL passthrough (wiki example 1 body, verbatim)
        r, data = call_tool_retry(
            s,
            "search_dsl",
            {
                "tribunal": "trf1",
                "body": {"query": {"match": {"numeroProcesso": "00008323520184013202"}}},
            },
            expect=lambda d: isinstance(d, dict) and d.get("hits", {}).get("total", {}).get("value") == 1,
        )
        check(
            "search_dsl raw passthrough",
            data.get("hits", {}).get("total", {}).get("value") == 1,
            text_of(r)[:200],
        )

        # pagination round-trip with search_after
        r, data = call_tool_retry(
            s,
            "search_cases",
            {
                "tribunal": "tjms",
                "filters": {"filing_date_from": "2024-01-01", "filing_date_until": "2024-01-02"},
                "size": 2,
                "fields": ["numeroProcesso"],
            },
            expect=lambda d: isinstance(d, dict) and "next_page" in d,
        )
        if data and "next_page" in data:
            cursor = data["next_page"]["search_after"]
            r2, data2 = call_tool_retry(
                s,
                "search_cases",
                {
                    "tribunal": "tjms",
                    "filters": {"filing_date_from": "2024-01-01", "filing_date_until": "2024-01-02"},
                    "size": 2,
                    "fields": ["numeroProcesso"],
                    "search_after": cursor,
                },
                expect=lambda d: isinstance(d, dict) and len(d.get("cases", [])) > 0,
            )
            ids1 = [h["id"] for h in data["cases"]]
            ids2 = [h["id"] for h in (data2 or {}).get("cases", [])]
            check(
                "search_after pagination advances",
                len(ids2) > 0 and not set(ids1) & set(ids2),
                f"{ids1} vs {ids2}",
            )
        else:
            check("search_after pagination advances", False, "no next_page returned")

    s.close()
    print(f"\n== {ok} passed, {fail} failed ==")
    sys.exit(1 if fail else 0)


if __name__ == "__main__":
    main()
