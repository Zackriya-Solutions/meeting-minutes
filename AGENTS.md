# Custom Agent Rules for Ornith-9B

## Core Execution Scaffold Overrides
* **Low Memory Constraint:** You are running on a limited 4GB VRAM hardware node. Long terminal dumps destroy your context cache.
* **Command Output Rules:** When invoking the `bash` tool to run tasks that generate large data logs (e.g., test suites, build outputs, or long loops), you MUST intercept and redirect the output to prevent IDE truncation panic.
* **Prohibited Actions:** NEVER execute heavy commands raw. Avoid running `npm test`, `cargo build`, or directory scans without filters.
* **Allowed Execution Patterns:** Always route bulky streams to disk, then read small diagnostic tails. Use this exact syntax structure:
  `[COMMAND] > execution.log 2>&1 || head -n 50 execution.log`

## graphify

This project has a knowledge graph at graphify-out/ with god nodes, community structure, and cross-file relationships.

When the user types `/graphify`, use the installed graphify skill or instructions before doing anything else.

Rules:
- For codebase questions, first run `graphify query "<question>"` when graphify-out/graph.json exists. Use `graphify path "<A>" "<B>"` for relationships and `graphify explain "<concept>"` for focused concepts. These return a scoped subgraph, usually much smaller than GRAPH_REPORT.md or raw grep output.
- Dirty graphify-out/ files are expected after hooks or incremental updates; dirty graph files are not a reason to skip graphify. Only skip graphify if the task is about stale or incorrect graph output, or the user explicitly says not to use it.
- If graphify-out/wiki/index.md exists, use it for broad navigation instead of raw source browsing.
- Read graphify-out/GRAPH_REPORT.md only for broad architecture review or when query/path/explain do not surface enough context.
- After modifying code, run `graphify update .` to keep the graph current (AST-only, no API cost).
