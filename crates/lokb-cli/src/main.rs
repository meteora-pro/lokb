mod store;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "lokb", about = "Local Offline Knowledge Base")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Manage data sources
    Source {
        #[command(subcommand)]
        command: SourceCommands,
    },
    /// Search across all sources
    Search {
        /// Search query
        query: String,
        /// Output format
        #[arg(long, default_value = "text")]
        format: String,
        /// Search mode: quick (top 5), normal (top 20), deep (top 50)
        #[arg(long, default_value = "normal")]
        mode: String,
        /// Max results
        #[arg(long)]
        limit: Option<usize>,
        /// Filter by source name
        #[arg(long)]
        source: Option<String>,
        /// Search only personal sources
        #[arg(long)]
        personal_only: bool,
        /// Search only public sources
        #[arg(long)]
        public_only: bool,
    },
    /// Read a document
    Read {
        /// Document reference (source:document_id)
        doc_ref: String,
        /// Section to read
        #[arg(long)]
        section: Option<String>,
    },
    /// Storage management
    Storage {
        #[command(subcommand)]
        command: StorageCommands,
    },
    /// Quick fact lookup (entity + relations, falls back to search)
    Lookup {
        /// Query (e.g. "population of Paris", "capital of France")
        query: String,
        /// Output format
        #[arg(long, default_value = "text")]
        format: String,
    },
    /// Enrich existing source with LLM (summarize, etc.)
    Enrich {
        /// Source name
        source: String,
        /// Enrichment step to run
        #[arg(long, default_value = "summarize")]
        step: String,
        /// LLM backend spec (skip, ollama:model, openai:url:model)
        #[arg(long, default_value = "ollama:phi3")]
        llm: String,
        /// Max documents to process (0 = all)
        #[arg(long, default_value = "0")]
        limit: usize,
    },
    /// Look up an entity in the knowledge graph
    Entity {
        /// Entity name
        name: String,
        /// Show relations
        #[arg(long)]
        relations: bool,
        /// Show documents mentioning this entity
        #[arg(long)]
        documents: bool,
        /// Output format
        #[arg(long, default_value = "text")]
        format: String,
    },
    /// Start HTTP API server
    Serve {
        /// Port number
        #[arg(long, default_value = "7890")]
        port: u16,
    },
    /// Export knowledge base
    Export {
        /// Output file path
        output: String,
        /// Include personal data
        #[arg(long)]
        include_personal: bool,
    },
}

#[derive(Subcommand)]
enum SourceCommands {
    /// Add a new data source
    Add {
        /// Source name
        name: String,
        /// Path to raw data
        #[arg(long)]
        raw: String,
        /// Data format
        #[arg(long)]
        format: String,
        /// Data class (public/personal)
        #[arg(long)]
        class: String,
        /// Output format (text or json for machine-readable metrics)
        #[arg(long, default_value = "text")]
        output: String,
    },
    /// Update an existing source with new data
    Update {
        /// Source name
        name: String,
        /// Path to raw data
        #[arg(long)]
        raw: String,
    },
    /// Show detailed source status
    Status {
        /// Source name
        name: String,
        /// Output format
        #[arg(long, default_value = "text")]
        format: String,
    },
    /// Delete a source and all its data
    Delete {
        /// Source name
        name: String,
    },
    /// List all sources
    List {
        /// Output format
        #[arg(long, default_value = "text")]
        format: String,
    },
}

#[derive(Subcommand)]
enum StorageCommands {
    /// Show storage status
    Status {
        /// Output format
        #[arg(long, default_value = "text")]
        format: String,
    },
}

fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Source { command } => run_source(command),
        Commands::Search {
            query,
            format,
            mode,
            limit,
            source,
            personal_only,
            public_only,
        } => run_search(
            &query,
            &format,
            &mode,
            limit,
            source.as_deref(),
            personal_only,
            public_only,
        ),
        Commands::Read { doc_ref, section } => run_read(&doc_ref, section.as_deref()),
        Commands::Storage { command } => run_storage(command),
        Commands::Enrich {
            source,
            step,
            llm,
            limit,
        } => run_enrich(&source, &step, &llm, limit),
        Commands::Lookup { query, format } => run_lookup(&query, &format),
        Commands::Entity {
            name,
            relations,
            documents,
            format,
        } => run_entity(&name, relations, documents, &format),
        Commands::Serve { port } => run_serve(port),
        Commands::Export {
            output,
            include_personal,
        } => run_export(&output, include_personal),
    };

    if let Err(e) = result {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}

fn run_source(command: SourceCommands) -> Result<(), Box<dyn std::error::Error>> {
    match command {
        SourceCommands::Add {
            name,
            raw,
            format,
            class,
            output,
        } => {
            let metrics = store::add_source(&name, &raw, &format, &class)?;
            if output == "json" {
                println!("{}", serde_json::to_string_pretty(&metrics)?);
            } else {
                println!("Source '{}' added successfully", name);
                println!(
                    "  Documents: {}  Chunks: {}",
                    metrics.documents_processed, metrics.chunks_created
                );
                println!(
                    "  Optimize:  {:.1}s ({} → {} bytes, {:.1}x compression)",
                    metrics.optimize_time_ms as f64 / 1000.0,
                    metrics.raw_input_bytes,
                    metrics.optimized_bytes,
                    metrics.compression_ratio
                );
                println!(
                    "  Enrichment: {:.1}s (FTS index: {} bytes)",
                    metrics.enrichment_time_ms as f64 / 1000.0,
                    metrics.fts_index_bytes
                );
                println!("  Total: {:.1}s", metrics.total_time_ms as f64 / 1000.0);
            }
        }
        SourceCommands::Status { name, format } => {
            let status = store::source_status(&name)?;
            if format == "json" {
                println!("{}", serde_json::to_string_pretty(&status)?);
            } else {
                println!("Source: {}", status.name);
                println!("  Format:     {}", status.format);
                println!("  Class:      {}", status.class);
                println!("  Documents:  {}", status.document_count);
                println!("  Content:    {} bytes", status.content_bytes);
                println!("  Created:    {}", status.created_at);
            }
        }
        SourceCommands::Delete { name } => {
            store::delete_source(&name)?;
            println!("Source '{}' deleted", name);
        }
        SourceCommands::Update { name, raw } => {
            let report = store::update_source(&name, &raw)?;
            println!("Source '{}' updated:", name);
            println!("  New:       {}", report.new_count);
            println!("  Changed:   {}", report.changed_count);
            println!("  Unchanged: {}", report.unchanged_count);
            println!("  Deleted:   {}", report.deleted_count);
        }
        SourceCommands::List { format } => {
            let sources = store::list_sources()?;
            if format == "json" {
                let output = serde_json::json!({ "sources": sources });
                println!("{}", serde_json::to_string_pretty(&output)?);
            } else if sources.is_empty() {
                println!("No sources configured.");
            } else {
                for s in &sources {
                    println!(
                        "{:<20} {:<15} {:<10} {} doc(s)",
                        s.name, s.format, s.class, s.document_count
                    );
                }
            }
        }
    }
    Ok(())
}

fn run_search(
    query: &str,
    format: &str,
    mode: &str,
    limit: Option<usize>,
    source: Option<&str>,
    personal_only: bool,
    public_only: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let max_results = limit.unwrap_or(match mode {
        "quick" => 5,
        "deep" => 50,
        _ => 20, // normal
    });
    let results = store::search(query, max_results, source, personal_only, public_only)?;

    if format == "json" {
        let output = serde_json::json!({
            "query": query,
            "mode": mode,
            "results": results,
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else if results.is_empty() {
        println!("No results found.");
    } else {
        for r in &results {
            println!("--- {} [{}] (score: {:.2}) ---", r.title, r.source, r.score);
            println!("{}\n", r.snippet);
        }
    }
    Ok(())
}

fn run_read(doc_ref: &str, section: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
    let content = store::read_document(doc_ref)?;
    match section {
        Some(section_name) => {
            let section_header = format!("## {}", section_name);
            let mut in_section = false;
            let mut output = String::new();
            for line in content.lines() {
                if line.trim() == section_header || line.trim() == format!("### {}", section_name) {
                    in_section = true;
                    output.push_str(line);
                    output.push('\n');
                    continue;
                }
                if in_section {
                    if line.starts_with("## ") {
                        break;
                    }
                    output.push_str(line);
                    output.push('\n');
                }
            }
            if output.is_empty() {
                eprintln!("Section '{}' not found", section_name);
                std::process::exit(1);
            }
            print!("{output}");
        }
        None => print!("{content}"),
    }
    Ok(())
}

fn run_storage(command: StorageCommands) -> Result<(), Box<dyn std::error::Error>> {
    match command {
        StorageCommands::Status { format } => {
            let status = store::storage_status()?;
            if format == "json" {
                println!("{}", serde_json::to_string_pretty(&status)?);
            } else {
                println!("LAYERS");
                for l in &status.layers {
                    println!("  {:<10} {}", l.name, format_bytes(l.size_bytes));
                }
                println!("  {:<10} {}", "total", format_bytes(status.total_bytes));
                if !status.sources.is_empty() {
                    println!("\nSOURCES");
                    for s in &status.sources {
                        println!(
                            "  {:<20} {:<10} {:>6} docs  {}",
                            s.name,
                            s.class,
                            s.documents,
                            format_bytes(s.content_bytes)
                        );
                    }
                }
                if status.entity_count > 0 || status.relation_count > 0 {
                    println!("\nKNOWLEDGE GRAPH");
                    println!("  Entities:  {}", status.entity_count);
                    println!("  Relations: {}", status.relation_count);
                }
            }
        }
    }
    Ok(())
}

fn format_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.1} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

fn run_serve(port: u16) -> Result<(), Box<dyn std::error::Error>> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(lokb_serve::serve(port))?;
    Ok(())
}

fn run_enrich(
    source: &str,
    step: &str,
    llm_spec: &str,
    limit: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let count = store::enrich_source(source, step, llm_spec, limit)?;
    println!("Enriched {count} documents with step '{step}'");
    Ok(())
}

fn run_lookup(query: &str, format: &str) -> Result<(), Box<dyn std::error::Error>> {
    let result = store::fact_lookup(query)?;
    if format == "json" {
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else if let Some(answer) = result {
        println!("{}", answer.answer);
        if let Some(entity) = &answer.entity {
            println!("  Entity: {entity}");
        }
        if let Some(source) = &answer.source {
            println!("  Source: {source}");
        }
    } else {
        // Fallback to search
        let results = store::search(query, 3, None, false, false)?;
        if results.is_empty() {
            println!("No answer found for: {query}");
        } else {
            println!("No structured answer. Top search results:");
            for r in &results {
                println!("  [{}] {}", r.source, r.snippet);
            }
        }
    }
    Ok(())
}

fn run_entity(
    name: &str,
    show_relations: bool,
    show_documents: bool,
    format: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let result = store::entity_lookup(name, show_relations, show_documents)?;
    if format == "json" {
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
        match result {
            Some(entity) => {
                println!("Entity: {}", entity.canonical_name);
                if let Some(desc) = &entity.description {
                    println!("  {desc}");
                }
                println!("  Mentions: {}", entity.mention_count);
                if !entity.relations.is_empty() {
                    println!("\n  Relations:");
                    for rel in &entity.relations {
                        println!(
                            "    {} → {} ({:.0}%)",
                            rel.predicate,
                            rel.target_name,
                            rel.confidence * 100.0
                        );
                    }
                }
                if !entity.documents.is_empty() {
                    println!("\n  Documents:");
                    for doc in &entity.documents {
                        println!("    - {doc}");
                    }
                }
            }
            None => {
                // Try fuzzy search
                let suggestions = store::entity_search(name, 5)?;
                if suggestions.is_empty() {
                    println!("Entity '{name}' not found.");
                } else {
                    println!("Entity '{name}' not found. Did you mean:");
                    for s in &suggestions {
                        println!("  - {} ({} mentions)", s.canonical_name, s.mention_count);
                    }
                }
            }
        }
    }
    Ok(())
}

fn run_export(output: &str, include_personal: bool) -> Result<(), Box<dyn std::error::Error>> {
    store::export(output, include_personal)?;
    println!("Exported to {output}");
    Ok(())
}
