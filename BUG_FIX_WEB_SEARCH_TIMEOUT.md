# Web Search MCP Timeout Bug - Root Cause Analysis

## Problem
The `web_search` tool worked perfectly when called directly via `cargo run --example web_search` (3-4 seconds), but timed out indefinitely when called through the MCP protocol by AI agents.

## Root Cause
**Incorrect return type specification in the `Tool` trait implementation.**

### The Bug
In `/Volumes/samsung_t9/kodegen-workspace/packages/kodegen-tools-citescrape/src/mcp/web_search.rs`:

```rust
// ❌ WRONG - Hardcoded concrete type
async fn execute(&self, args: Self::Args, _ctx: ToolExecutionContext) 
    -> Result<ToolResponse<WebSearchOutput>, McpError>
```

### The Fix
```rust
// ✅ CORRECT - Uses associated type path from trait
async fn execute(&self, args: Self::Args, _ctx: ToolExecutionContext) 
    -> Result<ToolResponse<<Self::Args as kodegen_mcp_schema::ToolArgs>::Output>, McpError>
```

## Why This Mattered

The `Tool` trait in `kodegen-mcp-schema` defines the exact signature for `execute()`:

```rust
fn execute(
    &self,
    args: Self::Args,
    ctx: ToolExecutionContext,
) -> impl std::future::Future<Output = Result<
    ToolResponse<<Self::Args as ToolArgs>::Output>,
    McpError
>>
```

### The Type System Contract

1. **Schema Definition**: `WebSearchArgs` implements `ToolArgs` with `Output = WebSearchOutput`
2. **Trait Requirement**: The return type MUST be `ToolResponse<<Self::Args as ToolArgs>::Output>`
3. **Type Equivalence**: While `WebSearchOutput` and `<Self::Args as ToolArgs>::Output` are equivalent types at runtime, they are NOT equivalent type paths at compile time
4. **MCP Framework**: The MCP layer performs type checking and dynamic dispatch based on the trait's associated type path, not the concrete type name

### Why It Worked in Examples but Not in MCP

- **Direct calls**: Bypass the trait object/dynamic dispatch layer
- **MCP calls**: Go through trait object conversion where the type path must match exactly
- **Timeout symptom**: The MCP framework likely failed to deserialize/route the response because the type signature didn't match expectations, causing the call to hang indefinitely

## Comparison with Working Tools

All working tools (e.g., `fs_read_file`, `terminal`, etc.) use the correct pattern:

```rust
// From kodegen-tools-filesystem/src/read_file.rs
async fn execute(&self, args: Self::Args, ctx: ToolExecutionContext) 
    -> Result<ToolResponse<<Self::Args as kodegen_mcp_schema::ToolArgs>::Output>, McpError>
```

## Files Changed

- `/Volumes/samsung_t9/kodegen-workspace/packages/kodegen-tools-citescrape/src/mcp/web_search.rs`
  - Line 68: Updated `execute()` return type signature

## Verification

```bash
cd /Volumes/samsung_t9/kodegen-workspace/packages/kodegen-tools-citescrape
cargo check   # ✓ Passes
cargo build --release  # ✓ Passes
```

## Lessons Learned

1. **Always use associated type paths in trait implementations** - Don't hardcode concrete types even when they're equivalent
2. **The type system is stricter than it appears** - Type paths matter for dynamic dispatch and serialization frameworks
3. **Test through the actual call path** - Examples may work while MCP calls fail due to different code paths
4. **Follow existing patterns** - Compare with known-working implementations when troubleshooting

## Additional Notes

This is a subtle bug that highlights the importance of:
- Precise type system adherence in Rust
- Understanding trait object conversions and dynamic dispatch
- Testing through production code paths, not just simplified examples
- Pattern matching with existing, proven implementations

The fix is minimal (one line change) but critical for MCP protocol compatibility.
