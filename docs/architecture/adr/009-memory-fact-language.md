# ADR-009: Memory Fact Language (MFL)

**Status:** Proposed  
**Date:** 2026-04-16  
**Author:** andreymaznyak@gmail.com

## Context

lokb stores knowledge as Documents → Chunks with optional embeddings (ADR-004). This works well for static content (Wikipedia, books, code files) but not for **dynamic agent memory** — facts extracted from conversations that evolve over time, form relationships, and need compact representation for LLM context windows.

AI coding agents (DevBoyClaw, Claude Code, etc.) need a memory system that:

1. **Stores structured facts** — not plain text, but typed entities with attributes and relationships
2. **Supports multi-hop traversal** — "find everything related to KV cache" → follow links 2-3 hops deep
3. **Evolves over time** — facts get updated, merged, superseded as new information arrives
4. **Compresses efficiently** — 3-4x fewer tokens than natural language for same information
5. **Detects contradictions** — "user prefers Python" vs "user switched to Rust" → resolve by temporal ordering
6. **Contains executable knowledge** — not just data ("project uses Rust") but rules ("when migration → assert snake_case") and templates ("for commit → format type(scope): desc")

Existing approaches (studied):

- **SimpleMem** (arXiv:2601.02553): 30x retrieval token reduction via structured compression. Multi-view indexing (semantic + lexical + symbolic). Consolidation merge by cosine > 0.95. But: flat units, no links, no evolution.
- **A-MEM** (arXiv:2502.12110, NeurIPS 2025): Zettelkasten-inspired notes with bidirectional links. Write-heavy (3 LLM calls), read-cheap (0 LLM calls). Memory evolution updates existing notes. But: flat content, binary links, no structure.
- **Graphiti/Zep**: Bi-temporal knowledge graph with `valid_from`/`valid_until`. Contradiction = invalidate old. But: no formal language, no code-as-data.
- **Generative Agents** (Park et al., 2023): Retrieval formula `α_recency × recency + α_importance × importance + α_relevance × relevance`. Proven effective.

None provide a **formal language** for knowledge representation that combines structured data, typed relationships, executable rules, and compiler-style optimization.

## Decision

Introduce **Memory Fact Language (MFL)** — a formal language for representing, storing, and transforming knowledge in lokb. MFL treats facts as source code and uses compiler-style optimization passes (dream cycle) to keep the knowledge base compact, consistent, and connected.

### Philosophy

```
Raw Events (NL text, immutable)     = Source code (.rs files)
MFL Facts (structured, mutable)     = Intermediate Representation (AST/IR)
Breadcrumbs / Recall results        = Compiled output (binary)
Dream Cycle                         = Compiler optimization passes
```

Facts are programs describing knowledge. The dream cycle compiles experience into patterns, rules, and abstractions — like a compiler optimizes code.

### Prescribed ontology

Fixed set of entity types for coding agent domain (extensible per-consumer):

| Type | Semantics | Example |
|------|-----------|---------|
| `User` | Person: role, skills, preferences | `@User(U1) { role=engineer; skills=[Rust,TS] }` |
| `Team` | Group: conventions, processes, stack | `@Team(T1) { lang.docs=ru; lang.code=en }` |
| `Project` | Codebase: architecture, decisions | `@Project(F42) { arch=L1→L2→L3→L4→L5 }` |
| `Task` | Work item: status, branch, deps | `@Task(T15) { id=DEV-768; status=in_progress }` |
| `Code` | Pattern, convention, file knowledge | `@Code(C50) { pattern=OnceLock_Hook }` |
| `Tool` | Tool config, usage, limits | `@Tool(TL1) { name=shell; timeout=120s }` |
| `Event` | Temporal: meeting, incident, release | `@Event(E1) { type=release; v=0.6.9 }` |
| `Rule` | Conditional: when X → do Y | `@Rule(R3) { when migration { assert snake_case } }` |
| `Template` | Repeatable: format for X | `@Template(T2) { for commit { format="..." } }` |

### Link types

| Type | Semantics | Directionality |
|------|-----------|----------------|
| `→related` | Semantic connection | Bidirectional |
| `→supersedes` | Temporal: new replaces old | Source → Target (target gets valid_until) |
| `→requires` | Dependency | Directed |
| `→part_of` | Hierarchy: A is component of B | Directed |
| `→caused_by` | Causal | Directed |
| `→decided_in` | Traceability to event | Directed |
| `→co_recalled` | Implicit: recalled together | Bidirectional, strength grows |

### Grammar (PEG)

```peg
// === Top level ===
Factsheet   = Fact+
Fact        = Header Body Links? Validity?

// === Header: metadata on one line ===
Header      = "@" EntityType "(" ID ")" Scope Importance Date
EntityType  = "User" / "Team" / "Project" / "Task" / "Code" / "Tool" / "Event" / "Rule" / "Template"
Scope       = "§" ScopePath
ScopePath   = "global" / ID ("/" ID)*           // §meteora/zeroclaw
Importance  = "!" Float                          // !0.95
Date        = "#" IsoDate                        // #2026-04-14

// === Body: recursive blocks ===
Body        = "{" Statement+ "}"
Statement   = Assignment / NamedBlock / ListBlock / Chain
            / WhenBlock / ForBlock / AssertStmt / WarnStmt / RecallStmt

// Data constructs
Assignment  = Path "=" Value ";"
NamedBlock  = ID Body                            // recursive nesting
ListBlock   = ID "{" Item+ "}"                   // list of items
Chain       = Node ("→" Node)+ ";"               // directed sequence
Item        = ValueExpr ("@" Location)? ("#" Date)? ";"

// Code constructs (Rule/Template entity types)
WhenBlock   = "when" Condition Body
ForBlock    = "for" ID Body
AssertStmt  = "assert" Path "=" Value ";"
WarnStmt    = "warn" String ";"
RecallStmt  = "recall" Scope ";"

// Conditions
Condition   = Path "matches" Pattern ("|" Pattern)*
            / Path Comparator Value
            / Condition "&&" Condition
            / "NOT" Condition

// Values
Value       = String / Number / Duration / Bool / List / Ref
Duration    = Number TimeUnit                    // 5m, 24h, 30d
TimeUnit    = "s" / "m" / "h" / "d"
List        = "[" Value ("," Value)* "]"
Ref         = "$" ID                             // reference to another fact
Path        = ID ("." ID)* ("[" Int "]")?

// === Links ===
Links       = ("→" LinkType ":" TargetList)+
TargetList  = ID ("," ID)*
LinkType    = "related" / "supersedes" / "requires" / "part_of"
            / "caused_by" / "decided_in"

// === Temporal validity ===
Validity    = "~valid" Date (".." Date)?         // ~valid 2026-04 or ~valid 2026-03..2026-04
```

### Three levels of facts

**Data fact** — what exists:
```
@Project(F042) §meteora/zeroclaw !0.95 #2026-04-14
{ arch.levels = L1:Conv → L2:TODO(phases=[R,E,M]) → L3:Session(trigger=compact)
    → L4:KVCache(ttl=5m, strategy=frozen) → L5:ToolLoop;
  stack = [Rust, SQLite, WebSocket];
}
→related: F002, F005
```

**Pattern fact** — what repeats (abstracted from multiple data facts):
```
@Code(C050) §meteora/zeroclaw !0.88 #2026-04-15
{ pattern OnceLock_Hook {
    signature = "register_*_fn(Box<dyn Fn(...)>)";
    convention = "OnceLock.set() at startup, .get() at runtime";
    instances {
      tools_replace     @ agent/loop_.rs      #2026-04-15;
      observer_factory  @ observability/mod.rs #2026-04-12;
      system_prompt     @ channels/mod.rs      #2026-04-13;
    }
  }
}
→supersedes: C001, C002, C003
```

**Rule/Template fact** — what to do (executable knowledge):
```
@Rule(R003) §meteora/zeroclaw !0.90 #2026-04-10
{ when context.task matches "migration" | "database"
    && NOT context.task matches "redis" | "cache_key" {
    assert column_names = snake_case;
    warn "TypeORM SnakeNamingStrategy — never camelCase in ALTER TABLE";
    example_bad  = 'ADD "projectId" uuid';
    example_good = 'ADD "project_id" uuid';
  }
}
→related: C012
```

### Compression efficiency

| Technique | NL (before) | MFL (after) | Saving |
|-----------|-------------|-------------|--------|
| Numeric literals | "five minutes" (2 tok) | `5m` (1 tok) | 2x |
| References | "the architecture from fact F042" (8 tok) | `$F042` (1 tok) | 8x |
| Typed links | "is related to the KV cache decision" (8 tok) | `→related: F002` (2 tok) | 4x |
| Lists | "Research, Execution, and Management" (5 tok) | `[R,E,M]` (2 tok) | 2.5x |
| Chains | "L1 contains L2 which contains L3..." (12 tok) | `L1→L2→L3→L4→L5` (3 tok) | 4x |
| Pattern merge | 4 flat facts × ~35 tok | 1 structured × ~50 tok | 2.8x |

Overall: NL → full MFL = **1.6-2x**, NL → MFL breadcrumb = **3-4x** compression.

### Bi-temporal model (from Graphiti)

Each fact carries:
- `created_at` — when extracted
- `valid_from` — when the fact became true
- `valid_until` — when invalidated (`None` = active)
- `confidence` — starts at importance value, decays with half-life 30 days

Contradiction: two active facts with same entity path + different value → newer wins, older gets `valid_until = now`. Both preserved for history.

### Multi-view indexing

Each fact indexed in 4 parallel views:

| View | What | How |
|------|------|-----|
| **Semantic** | `embedding(mfl_source + summary + keywords)` | Cosine similarity |
| **Lexical** | `mfl_source + summary` text | FTS5 / Tantivy BM25 |
| **Symbolic** | `entity_type, scope, importance, valid_until` | SQL WHERE |
| **Graph** | `fact_links` adjacency | Recursive CTE (multi-hop) |

### Retrieval scoring (adapted from Park et al.)

```
score(fact, query) =
    0.30 × cosine(fact.embedding, query.embedding) +
    0.25 × fact.importance +
    0.15 × exp(-0.10 × days_since(fact.last_accessed)) +
    0.15 × exp(-0.023 × days_since(fact.valid_from)) +
    0.15 × graph_centrality(fact)
```

Weights configurable. `graph_centrality` = normalized active link count.

### Dream cycle: compiler optimization passes

Dream cycle transforms the MFL AST forest — both structure and content can change.

| Pass | What | Compiler analogy | Frequency |
|------|------|-----------------|-----------|
| **Deduplicate** | cosine > 0.95 + same type + same scope → merge | CSE | 6h |
| **Pattern Recognition** | N facts with repeating key → NamedBlock with ListBlock | Constant Folding | 6h |
| **Hierarchy Detection** | related + shared scope + content⊂ → nesting | Tree Restructuring | 24h |
| **Contradiction Resolution** | same path + different value → newer wins | Type Checking | 6h |
| **Temporal Compaction** | supersedes chain A→B→C→D → keep D, archive rest | Dead Code Elimination | 24h |
| **Memory Evolution** | new fact → update keywords/tags of linked facts | Inlining | 6h |
| **Rule Refinement** | observe agent errors → add conditions to Rules | Profile-Guided Optimization | 24h |
| **Importance Rebalancing** | recalculate from hit_count + links + freshness | Register Allocation | 24h |
| **Breadcrumb Recompilation** | project AST → updated breadcrumb cache | Code Generation | 6h |

Example — Pattern Recognition pass:

Before (3 flat facts):
```
@Code(C001) §zeroclaw !0.70 { pattern=OnceLock; use=hook_reg; location=loop_.rs }
@Code(C002) §zeroclaw !0.65 { pattern=OnceLock; use=observer; location=observability/mod.rs }
@Code(C003) §zeroclaw !0.65 { pattern=OnceLock; use=sysprompt; location=channels/mod.rs }
```

After (1 structured fact — AST shape changed):
```
@Code(C050) §zeroclaw !0.85 #2026-04-15
{ pattern OnceLock_Hook {
    instances {
      hook_reg  @ loop_.rs;
      observer  @ observability/mod.rs;
      sysprompt @ channels/mod.rs;
    }
  }
}
→supersedes: C001, C002, C003
```

### Storage schema

```sql
CREATE TABLE mfl_facts (
    id            TEXT PRIMARY KEY,
    entity_type   TEXT NOT NULL,
    scope_path    TEXT NOT NULL,        -- "meteora/zeroclaw"
    mfl_source    TEXT NOT NULL,        -- raw MFL notation
    summary       TEXT NOT NULL,        -- <80 chars for breadcrumbs
    keywords      TEXT,                 -- JSON array
    tags          TEXT,                 -- JSON array
    importance    REAL NOT NULL,
    confidence    REAL NOT NULL,
    embedding     BLOB,
    created_at    TEXT NOT NULL,
    valid_from    TEXT NOT NULL,
    valid_until   TEXT,
    hit_count     INTEGER DEFAULT 0,
    last_accessed_at TEXT,
    source_event_id  TEXT,
    merged_from      TEXT,             -- JSON array of superseded IDs
    cluster_id       TEXT
);

CREATE TABLE mfl_fact_links (
    source_id   TEXT NOT NULL REFERENCES mfl_facts(id),
    target_id   TEXT NOT NULL REFERENCES mfl_facts(id),
    link_type   TEXT NOT NULL,
    strength    REAL NOT NULL DEFAULT 1.0,
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL,
    PRIMARY KEY (source_id, target_id, link_type)
);

CREATE VIRTUAL TABLE mfl_facts_fts USING fts5(id, mfl_source, summary, keywords);
```

### Integration with lokb architecture

MFL fits into lokb's four-layer storage (ADR-001):

| lokb Layer | MFL mapping |
|------------|-------------|
| **RAW** | Raw events (conversation turns, git events) — append-only, immutable |
| **OPTIMIZED** | MFL facts — compiled from raw events, mutable by dream cycle |
| **DERIVED** | Indexes: embeddings, FTS, graph links, breadcrumb cache |
| **CACHE** | Projected breadcrumbs, pre-computed subgraphs |

MFL facts map to lokb's core entities (ADR-004):
- `mfl_facts` ≈ `Document` (OPTIMIZED layer, with MFL as content format)
- `mfl_fact_links` ≈ `Relation` (knowledge graph edges)
- Keywords/tags ≈ `Entity` links (extracted concepts)
- Breadcrumbs ≈ CACHE layer projections

New pipeline type for ADR-002:
- **MFL Consolidation Pipeline** (RAW events → OPTIMIZED MFL facts): LLM-based extraction + MFL parsing
- **MFL Dream Pipeline** (OPTIMIZED → OPTIMIZED): AST transformation passes (no LLM for most passes)
- **MFL Projection Pipeline** (OPTIMIZED → DERIVED/CACHE): breadcrumbs, indexes, subgraph cache

## Consequences

**Pros:**
- Formal language with grammar → parseable, validatable, transformable
- 3-4x token compression vs NL for breadcrumbs
- Code-as-data: rules and templates evolve with experience
- Dream cycle = compiler passes → deterministic optimization
- Multi-view indexing → flexible retrieval (semantic + lexical + symbolic + graph)
- Bi-temporal → clean contradiction handling
- Fits lokb's existing architecture (4 layers, pipelines, entities)

**Cons:**
- Custom parser needed (PEG, Rust implementation)
- LLM must learn to generate MFL (few-shot prompting in consolidation)
- Grammar evolution requires versioning (backward-compatible additions)
- Dream cycle passes are complex (9 passes, mix of algorithmic + LLM-assisted)
- Testing: need comprehensive test suite for parser + each dream cycle pass

## Alternatives considered

1. **Plain text facts (upstream zeroclaw):** Simple but snowball-prone, no structure, no compression
2. **JSON facts:** Parseable but verbose (keys repeated), no chain notation, no code constructs
3. **RDF/OWL:** Too formal, LLMs can't generate reliably, verbose
4. **Datalog:** Powerful inference but facts are immutable — we need mutable AST with structural transformations
5. **Just use A-MEM:** Good linking but flat content, no compression, no code-as-data

MFL combines the best: structured like JSON, compact like DSL, executable like code, transformable like compiler IR.

## Open research questions

1. **MFL parser implementation** — pest (PEG) vs nom (combinator) vs tree-sitter (incremental)?
2. **LLM generation reliability** — how reliably can GLM-5.1 generate valid MFL? Fallback strategy?
3. **Dream cycle scheduling** — fixed intervals (6h/24h) or event-driven (after N new facts)?
4. **Embedding model** — lokb default (multilingual-e5-small, 384d) vs agent-specific?
5. **Rule execution model** — interpreted by LLM at recall time vs compiled to programmatic checks?
