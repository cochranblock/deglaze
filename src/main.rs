// Unlicense — cochranblock.org
// Contributors: GotEmCoach, KOVA, Claude Opus 4.6

use anyhow::{Context, Result};
use clap::Parser;
use std::collections::HashMap;
use std::path::PathBuf;

/// deglaze — JS Budget Auditor for WebAssembly.
/// Analyzes wasm-bindgen JS glue, categorizes every function,
/// maps each to the WASM spec proposal that would eliminate it.
#[derive(Parser)]
#[command(name = "deglaze", version, about)]
enum Cli {
    /// Audit a wasm-bindgen JS glue file
    Audit {
        /// Path to the wasm-bindgen generated .js file
        js_file: PathBuf,
        /// Output format: text, json, or markdown
        #[arg(short, long, default_value = "text")]
        format: String,
    },
    /// Generate JS_BUDGET.md from audit results
    Budget {
        /// Path to the wasm-bindgen generated .js file
        js_file: PathBuf,
        /// Output path for JS_BUDGET.md
        #[arg(short, long, default_value = "JS_BUDGET.md")]
        output: PathBuf,
    },
    /// Compare two JS glue files (before/after)
    Diff {
        /// Previous JS glue file
        old: PathBuf,
        /// Current JS glue file
        new: PathBuf,
    },
}

/// Category of a JS glue function — what browser API it bridges
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize)]
enum Category {
    Dom,
    Fetch,
    Storage,
    Events,
    Timer,
    Console,
    Canvas,
    WebGpu,
    WebSocket,
    Crypto,
    Url,
    TextCodec,
    WasmLoader,
    MemoryBridge,
    TypeConversion,
    ErrorHandling,
    Other,
}

impl std::fmt::Display for Category {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Dom => write!(f, "DOM"),
            Self::Fetch => write!(f, "Fetch/HTTP"),
            Self::Storage => write!(f, "Storage"),
            Self::Events => write!(f, "Events"),
            Self::Timer => write!(f, "Timer"),
            Self::Console => write!(f, "Console"),
            Self::Canvas => write!(f, "Canvas"),
            Self::WebGpu => write!(f, "WebGPU"),
            Self::WebSocket => write!(f, "WebSocket"),
            Self::Crypto => write!(f, "Crypto"),
            Self::Url => write!(f, "URL"),
            Self::TextCodec => write!(f, "TextEncoder/Decoder"),
            Self::MemoryBridge => write!(f, "WASM Memory Bridge"),
            Self::TypeConversion => write!(f, "Type Conversion"),
            Self::ErrorHandling => write!(f, "Error Handling"),
            Self::WasmLoader => write!(f, "WASM Loader"),
            Self::Other => write!(f, "Other"),
        }
    }
}

/// Which WASM spec proposal would eliminate this JS function
#[derive(Debug, Clone, serde::Serialize)]
struct Proposal {
    name: &'static str,
    phase: u8,
    url: &'static str,
    status: &'static str,
}

const PROPOSALS: &[(&str, Proposal)] = &[
    ("DOM", Proposal {
        name: "Component Model",
        phase: 1,
        url: "https://github.com/WebAssembly/component-model",
        status: "Phase 1 — expected to advance after WASI 0.3",
    }),
    ("Fetch/HTTP", Proposal {
        name: "JS Promise Integration (JSPI)",
        phase: 4,
        url: "https://github.com/WebAssembly/js-promise-integration",
        status: "Phase 4 — Chrome ships, Firefox flagged, Safari assigned",
    }),
    ("Storage", Proposal {
        name: "Component Model + WASI",
        phase: 1,
        url: "https://github.com/WebAssembly/WASI",
        status: "Phase 1 — WASI 0.2 stable, browser support pending",
    }),
    ("Events", Proposal {
        name: "Component Model",
        phase: 1,
        url: "https://github.com/WebAssembly/component-model",
        status: "Phase 1 — DOM event binding requires Component Model",
    }),
    ("Timer", Proposal {
        name: "JS Promise Integration (JSPI)",
        phase: 4,
        url: "https://github.com/WebAssembly/js-promise-integration",
        status: "Phase 4 — setTimeout/setInterval bridgeable via JSPI",
    }),
    ("Console", Proposal {
        name: "Component Model",
        phase: 1,
        url: "https://github.com/WebAssembly/component-model",
        status: "Phase 1 — console.log requires host binding",
    }),
    ("Canvas", Proposal {
        name: "Component Model",
        phase: 1,
        url: "https://github.com/WebAssembly/component-model",
        status: "Phase 1 — Canvas2D API requires host binding",
    }),
    ("WebGPU", Proposal {
        name: "Component Model",
        phase: 1,
        url: "https://github.com/WebAssembly/component-model",
        status: "Phase 1 — WebGPU API requires host binding",
    }),
    ("WebSocket", Proposal {
        name: "JS Promise Integration (JSPI)",
        phase: 4,
        url: "https://github.com/WebAssembly/js-promise-integration",
        status: "Phase 4 — async WebSocket bridgeable via JSPI",
    }),
    ("Crypto", Proposal {
        name: "Component Model + WASI Crypto",
        phase: 1,
        url: "https://github.com/WebAssembly/wasi-crypto",
        status: "Phase 1 — WASI crypto API in development",
    }),
    ("TextEncoder/Decoder", Proposal {
        name: "JS Primitive Builtins",
        phase: 2,
        url: "https://github.com/nicolo-ribaudo/tc39-proposal-wasm-js-string-builtins",
        status: "Phase 2 — native string operations without JS bridge",
    }),
    ("WASM Loader", Proposal {
        name: "ESM Integration",
        phase: 3,
        url: "https://github.com/WebAssembly/esm-integration",
        status: "Phase 3 — Deno ships, Node flagged, no browser yet",
    }),
    ("WASM Memory Bridge", Proposal {
        name: "Component Model",
        phase: 1,
        url: "https://github.com/WebAssembly/component-model",
        status: "Phase 1 — typed memory access without JS marshaling",
    }),
    ("Type Conversion", Proposal {
        name: "JS Primitive Builtins",
        phase: 2,
        url: "https://github.com/nicolo-ribaudo/tc39-proposal-wasm-js-string-builtins",
        status: "Phase 2 — native type coercion without JS",
    }),
    ("Error Handling", Proposal {
        name: "Exception Handling (shipped) + Component Model",
        phase: 5,
        url: "https://github.com/WebAssembly/exception-handling",
        status: "Phase 5 (exceptions shipped) — error propagation still needs Component Model",
    }),
];

/// A single JS glue function found in the wasm-bindgen output
#[derive(Debug, serde::Serialize)]
struct GlueFunction {
    name: String,
    line: usize,
    bytes: usize,
    category: Category,
}

/// Full audit result
#[derive(Debug, serde::Serialize)]
struct AuditResult {
    file: String,
    total_bytes: usize,
    total_functions: usize,
    functions: Vec<GlueFunction>,
    by_category: HashMap<String, CategorySummary>,
}

#[derive(Debug, serde::Serialize)]
struct CategorySummary {
    count: usize,
    bytes: usize,
    proposal: Option<String>,
    proposal_phase: Option<u8>,
    proposal_url: Option<String>,
    proposal_status: Option<String>,
}

fn categorize(name: &str, body: &str) -> Category {
    let combined = format!("{} {}", name.to_lowercase(), body.to_lowercase());

    if combined.contains("document.") || combined.contains("createelement")
        || combined.contains("appendchild") || combined.contains("innerhtml")
        || combined.contains("getelementby") || combined.contains("queryselector")
        || combined.contains("setattribute") || combined.contains("classlist")
        || combined.contains("style.") || combined.contains("node.")
        || combined.contains("element.")
    {
        return Category::Dom;
    }
    if combined.contains("fetch(") || combined.contains("xmlhttprequest")
        || combined.contains("response.") || combined.contains("request.")
        || combined.contains("headers.")
    {
        return Category::Fetch;
    }
    if combined.contains("localstorage") || combined.contains("sessionstorage")
        || combined.contains("indexeddb")
    {
        return Category::Storage;
    }
    if combined.contains("addeventlistener") || combined.contains("removeeventlistener")
        || combined.contains("dispatchevent") || combined.contains("event.")
    {
        return Category::Events;
    }
    if combined.contains("settimeout") || combined.contains("setinterval")
        || combined.contains("requestanimationframe")
    {
        return Category::Timer;
    }
    if combined.contains("console.") {
        return Category::Console;
    }
    if combined.contains("canvas") || combined.contains("getcontext")
        || combined.contains("bindtexture") || combined.contains("bindframebuffer")
    {
        return Category::Canvas;
    }
    if combined.contains("gpudevice") || combined.contains("gpubuffer")
        || combined.contains("gpuqueue") || combined.contains("gpucommand")
        || combined.contains("navigator.gpu")
    {
        return Category::WebGpu;
    }
    if combined.contains("websocket") {
        return Category::WebSocket;
    }
    if combined.contains("crypto.") || combined.contains("subtlecrypto") {
        return Category::Crypto;
    }
    if combined.contains("new url(") || combined.contains("url.") {
        return Category::Url;
    }
    if combined.contains("textencoder") || combined.contains("textdecoder") {
        return Category::TextCodec;
    }
    if combined.contains("webassembly.instantiate") || combined.contains("webassembly.compile")
        || combined.contains("webassembly.module") || combined.contains("__wbg_init")
        || combined.contains("__wbindgen_init")
    {
        return Category::WasmLoader;
    }
    if combined.contains("getobject") || combined.contains("addheapobject")
        || combined.contains("takeobject") || combined.contains("dropobject")
        || combined.contains("wbindgen_memory") || combined.contains("__wbg_buffer")
        || combined.contains("uint8array") || combined.contains("int32array")
        || combined.contains("float32array") || combined.contains("dataview")
    {
        return Category::MemoryBridge;
    }
    if combined.contains("__wbindgen_number") || combined.contains("__wbindgen_string")
        || combined.contains("__wbindgen_boolean") || combined.contains("__wbindgen_is_")
        || combined.contains("__wbindgen_bigint") || combined.contains("__wbindgen_in")
        || combined.contains("typeof")
    {
        return Category::TypeConversion;
    }
    if combined.contains("__wbindgen_throw") || combined.contains("__wbindgen_error")
        || combined.contains("stack") || combined.contains("error.")
    {
        return Category::ErrorHandling;
    }

    Category::Other
}

fn parse_glue_functions(source: &str) -> Vec<GlueFunction> {
    let mut functions = Vec::new();
    let lines: Vec<&str> = source.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim();

        // Match: function name(...) {  or  export function name(...) {
        // or: module.exports.name = function(...) {
        // or: const name = function(...) {
        let fn_name = if trimmed.starts_with("function ")
            || trimmed.starts_with("export function ")
            || trimmed.starts_with("async function ")
            || trimmed.starts_with("export async function ")
        {
            let after = trimmed
                .trim_start_matches("export ")
                .trim_start_matches("async ")
                .trim_start_matches("function ");
            after.split('(').next().map(|s| s.trim().to_string())
        } else if (trimmed.starts_with("const ") || trimmed.starts_with("let ")
            || trimmed.starts_with("var "))
            && trimmed.contains("function")
        {
            let after = trimmed
                .trim_start_matches("const ")
                .trim_start_matches("let ")
                .trim_start_matches("var ");
            after.split('=').next().map(|s| s.trim().to_string())
        } else {
            None
        };

        if let Some(name) = fn_name {
            if !name.is_empty() && trimmed.contains('{') {
                // Find the end of the function body
                let start = i;
                let mut depth = 0;
                let mut end = i;
                for j in i..lines.len() {
                    for ch in lines[j].chars() {
                        if ch == '{' { depth += 1; }
                        if ch == '}' { depth -= 1; }
                    }
                    if depth == 0 {
                        end = j;
                        break;
                    }
                }
                let body: String = lines[start..=end].join("\n");
                let bytes = body.len();
                let category = categorize(&name, &body);

                functions.push(GlueFunction {
                    name,
                    line: start + 1,
                    bytes,
                    category,
                });

                i = end + 1;
                continue;
            }
        }
        i += 1;
    }

    functions
}

fn get_proposal(category: &str) -> Option<&'static Proposal> {
    PROPOSALS.iter().find(|(cat, _)| *cat == category).map(|(_, p)| p)
}

fn run_audit(js_file: &PathBuf) -> Result<AuditResult> {
    let source = std::fs::read_to_string(js_file)
        .with_context(|| format!("reading {}", js_file.display()))?;

    let total_bytes = source.len();
    let functions = parse_glue_functions(&source);
    let total_functions = functions.len();

    let mut by_category: HashMap<String, CategorySummary> = HashMap::new();
    for func in &functions {
        let cat_name = func.category.to_string();
        let entry = by_category.entry(cat_name.clone()).or_insert_with(|| {
            let proposal = get_proposal(&cat_name);
            CategorySummary {
                count: 0,
                bytes: 0,
                proposal: proposal.map(|p| p.name.to_string()),
                proposal_phase: proposal.map(|p| p.phase),
                proposal_url: proposal.map(|p| p.url.to_string()),
                proposal_status: proposal.map(|p| p.status.to_string()),
            }
        });
        entry.count += 1;
        entry.bytes += func.bytes;
    }

    Ok(AuditResult {
        file: js_file.display().to_string(),
        total_bytes,
        total_functions,
        functions,
        by_category,
    })
}

fn print_text(result: &AuditResult) {
    println!("\n=== DEGLAZE — JS Budget Audit ===\n");
    println!("  File:       {}", result.file);
    println!("  Total JS:   {} bytes ({:.1} KB)", result.total_bytes, result.total_bytes as f64 / 1024.0);
    println!("  Functions:  {}", result.total_functions);

    println!("\n  --- BY CATEGORY ---\n");
    let mut cats: Vec<_> = result.by_category.iter().collect();
    cats.sort_by(|a, b| b.1.bytes.cmp(&a.1.bytes));

    println!("  {:<24} {:>5} {:>8}   {}", "Category", "Funcs", "Bytes", "Eliminated by");
    println!("  {}", "-".repeat(80));

    for (cat, summary) in &cats {
        let proposal = summary.proposal.as_deref().unwrap_or("—");
        let phase = summary.proposal_phase.map(|p| format!("(Phase {})", p)).unwrap_or_default();
        println!("  {:<24} {:>5} {:>7}B   {} {}",
            cat, summary.count, summary.bytes, proposal, phase);
    }

    let eliminable: usize = cats.iter()
        .filter(|(_, s)| s.proposal_phase.unwrap_or(0) >= 3)
        .map(|(_, s)| s.bytes)
        .sum();
    let total_fn_bytes: usize = cats.iter().map(|(_, s)| s.bytes).sum();

    println!("\n  --- BUDGET ---\n");
    println!("  Eliminable now (Phase 3+):   {} bytes ({:.1}%)",
        eliminable, eliminable as f64 / total_fn_bytes.max(1) as f64 * 100.0);
    println!("  Blocked on Component Model:  {} bytes",
        total_fn_bytes - eliminable);
    println!("  Target:                      0 bytes");
}

fn print_json(result: &AuditResult) {
    println!("{}", serde_json::to_string_pretty(result).unwrap_or_default());
}

fn write_budget_md(result: &AuditResult, output: &PathBuf) -> Result<()> {
    let mut md = String::new();
    md.push_str("# JS Budget — deglaze audit\n\n");
    md.push_str(&format!("**File:** `{}`  \n", result.file));
    md.push_str(&format!("**Total JS:** {} bytes ({:.1} KB)  \n", result.total_bytes, result.total_bytes as f64 / 1024.0));
    md.push_str(&format!("**Functions:** {}  \n", result.total_functions));
    md.push_str(&format!("**Date:** {}  \n\n", chrono_date()));

    md.push_str("## By Category\n\n");
    md.push_str("| Category | Functions | Bytes | Eliminated by | Phase |\n");
    md.push_str("|----------|-----------|-------|---------------|-------|\n");

    let mut cats: Vec<_> = result.by_category.iter().collect();
    cats.sort_by(|a, b| b.1.bytes.cmp(&a.1.bytes));

    for (cat, summary) in &cats {
        let proposal = summary.proposal.as_deref().unwrap_or("—");
        let phase = summary.proposal_phase.map(|p| p.to_string()).unwrap_or_else(|| "—".to_string());
        md.push_str(&format!("| {} | {} | {} | {} | {} |\n",
            cat, summary.count, summary.bytes, proposal, phase));
    }

    let eliminable: usize = cats.iter()
        .filter(|(_, s)| s.proposal_phase.unwrap_or(0) >= 3)
        .map(|(_, s)| s.bytes)
        .sum();
    let total_fn_bytes: usize = cats.iter().map(|(_, s)| s.bytes).sum();

    md.push_str("\n## Budget\n\n");
    md.push_str(&format!("- **Eliminable now (Phase 3+):** {} bytes ({:.1}%)\n",
        eliminable, eliminable as f64 / total_fn_bytes.max(1) as f64 * 100.0));
    md.push_str(&format!("- **Blocked on Component Model:** {} bytes\n", total_fn_bytes - eliminable));
    md.push_str("- **Target:** 0 bytes\n\n");

    md.push_str("## Proposals That Would Reduce This Budget\n\n");
    let mut seen = std::collections::HashSet::new();
    for (_, summary) in &cats {
        if let (Some(name), Some(url), Some(status)) = (&summary.proposal, &summary.proposal_url, &summary.proposal_status) {
            if seen.insert(name.clone()) {
                md.push_str(&format!("- **[{}]({})** — {}\n", name, url, status));
            }
        }
    }

    md.push_str("\n---\n\n*Generated by [deglaze](https://crates.io/crates/deglaze) — JS Budget Auditor for WebAssembly.*\n");

    std::fs::write(output, &md)
        .with_context(|| format!("writing {}", output.display()))?;
    println!("Wrote {}", output.display());
    Ok(())
}

fn chrono_date() -> String {
    // Simple date without pulling in chrono
    let output = std::process::Command::new("date")
        .arg("+%Y-%m-%d")
        .output()
        .ok();
    output
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli {
        Cli::Audit { js_file, format } => {
            let result = run_audit(&js_file)?;
            match format.as_str() {
                "json" => print_json(&result),
                "markdown" | "md" => {
                    let output = PathBuf::from("JS_BUDGET.md");
                    write_budget_md(&result, &output)?;
                }
                _ => print_text(&result),
            }
        }
        Cli::Budget { js_file, output } => {
            let result = run_audit(&js_file)?;
            write_budget_md(&result, &output)?;
        }
        Cli::Diff { old, new } => {
            let old_result = run_audit(&old)?;
            let new_result = run_audit(&new)?;
            println!("\n=== DEGLAZE — JS Budget Diff ===\n");
            let diff = new_result.total_bytes as i64 - old_result.total_bytes as i64;
            let sign = if diff > 0 { "+" } else { "" };
            println!("  Old: {} bytes ({:.1} KB) — {}",
                old_result.total_bytes, old_result.total_bytes as f64 / 1024.0, old_result.file);
            println!("  New: {} bytes ({:.1} KB) — {}",
                new_result.total_bytes, new_result.total_bytes as f64 / 1024.0, new_result.file);
            println!("  Diff: {}{} bytes", sign, diff);
            println!("  Functions: {} → {}", old_result.total_functions, new_result.total_functions);
        }
    }
    Ok(())
}
