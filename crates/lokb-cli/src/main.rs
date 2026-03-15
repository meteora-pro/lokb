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
            personal_only,
            public_only,
        } => run_search(&query, &format, personal_only, public_only),
        Commands::Read { doc_ref, section } => run_read(&doc_ref, section.as_deref()),
        Commands::Storage { command } => run_storage(command),
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
        } => {
            let metrics = store::add_source(&name, &raw, &format, &class)?;
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
    personal_only: bool,
    public_only: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let results = store::search(query, personal_only, public_only)?;
    if format == "json" {
        let output = serde_json::json!({
            "query": query,
            "results": results,
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else if results.is_empty() {
        println!("No results found.");
    } else {
        for r in &results {
            println!("--- {} [{}] ---", r.title, r.source);
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

fn run_export(output: &str, include_personal: bool) -> Result<(), Box<dyn std::error::Error>> {
    store::export(output, include_personal)?;
    println!("Exported to {output}");
    Ok(())
}
