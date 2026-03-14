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
            store::add_source(&name, &raw, &format, &class)?;
            println!("Source '{}' added successfully", name);
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
                        "{:<20} {:<15} {:<10} {} docs",
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
            let layers = store::storage_status()?;
            if format == "json" {
                let total: u64 = layers.iter().map(|l| l.size_bytes).sum();
                let output = serde_json::json!({
                    "layers": layers,
                    "total_bytes": total,
                });
                println!("{}", serde_json::to_string_pretty(&output)?);
            } else {
                for l in &layers {
                    println!("{:<10} {} bytes", l.name, l.size_bytes);
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
