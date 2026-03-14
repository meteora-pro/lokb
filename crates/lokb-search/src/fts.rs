use lokb_core::{Chunk, Result};
use std::path::Path;
use tantivy::collector::TopDocs;
use tantivy::query::QueryParser;
use tantivy::schema::*;
use tantivy::{Index, IndexReader, IndexWriter, ReloadPolicy, doc};

/// Tantivy-based full-text search index (BM25).
pub struct TantivyIndex {
    index: Index,
    reader: IndexReader,
    // Field handles
    f_chunk_id: Field,
    f_doc_id: Field,
    f_source_id: Field,
    f_source_name: Field,
    f_title: Field,
    f_text: Field,
    f_section: Field,
    f_content_type: Field,
    f_privacy_level: Field,
}

/// A search hit from the FTS index.
#[derive(Debug, Clone, serde::Serialize)]
pub struct FtsHit {
    pub chunk_id: String,
    pub doc_id: String,
    pub source_id: String,
    pub source_name: String,
    pub title: String,
    pub text: String,
    pub section: Option<String>,
    pub score: f32,
}

impl TantivyIndex {
    /// Open or create an index at the given path.
    pub fn open(path: &Path) -> Result<Self> {
        std::fs::create_dir_all(path)?;

        let mut schema_builder = Schema::builder();
        let f_chunk_id = schema_builder.add_text_field("chunk_id", STRING | STORED);
        let f_doc_id = schema_builder.add_text_field("doc_id", STRING | STORED);
        let f_source_id = schema_builder.add_text_field("source_id", STRING);
        let f_source_name = schema_builder.add_text_field("source_name", STRING | STORED);
        let f_title = schema_builder.add_text_field("title", TEXT | STORED);
        let f_text = schema_builder.add_text_field("text", TEXT | STORED);
        let f_section = schema_builder.add_text_field("section", STRING | STORED);
        let f_content_type = schema_builder.add_text_field("content_type", STRING);
        let f_privacy_level = schema_builder.add_u64_field("privacy_level", INDEXED | STORED);
        let schema = schema_builder.build();

        let index = Index::open_or_create(
            tantivy::directory::MmapDirectory::open(path)
                .map_err(|e| lokb_core::Error::Storage(e.to_string()))?,
            schema.clone(),
        )
        .map_err(|e| lokb_core::Error::Storage(e.to_string()))?;

        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommitWithDelay)
            .try_into()
            .map_err(|e| lokb_core::Error::Storage(e.to_string()))?;

        Ok(Self {
            index,
            reader,
            f_chunk_id,
            f_doc_id,
            f_source_id,
            f_source_name,
            f_title,
            f_text,
            f_section,
            f_content_type,
            f_privacy_level,
        })
    }

    /// Index a batch of chunks with their source metadata.
    pub fn index_chunks(&self, chunks: &[Chunk], source_name: &str, title: &str) -> Result<()> {
        let mut writer: IndexWriter = self
            .index
            .writer(50_000_000) // 50MB buffer
            .map_err(|e| lokb_core::Error::Storage(e.to_string()))?;

        for chunk in chunks {
            let mut doc = doc!(
                self.f_chunk_id => chunk.id.to_string(),
                self.f_doc_id => chunk.document_id.to_string(),
                self.f_source_id => chunk.source_id.to_string(),
                self.f_source_name => source_name,
                self.f_title => title,
                self.f_text => chunk.text.as_str(),
                self.f_content_type => format!("{:?}", chunk.content_type),
                self.f_privacy_level => chunk.privacy_level as u64,
            );
            if let Some(section) = &chunk.section_path {
                doc.add_text(self.f_section, section);
            }
            writer
                .add_document(doc)
                .map_err(|e| lokb_core::Error::Storage(e.to_string()))?;
        }

        writer
            .commit()
            .map_err(|e| lokb_core::Error::Storage(e.to_string()))?;
        self.reader
            .reload()
            .map_err(|e| lokb_core::Error::Storage(e.to_string()))?;

        Ok(())
    }

    /// Search for chunks matching the query.
    pub fn search(
        &self,
        query_str: &str,
        limit: usize,
        source_filter: Option<&str>,
        personal_only: bool,
        public_only: bool,
    ) -> Result<Vec<FtsHit>> {
        let searcher = self.reader.searcher();
        let query_parser = QueryParser::for_index(&self.index, vec![self.f_text, self.f_title]);
        let query = query_parser
            .parse_query(query_str)
            .map_err(|e| lokb_core::Error::Storage(e.to_string()))?;

        let top_docs = searcher
            .search(&query, &TopDocs::with_limit(limit * 3)) // fetch more, filter after
            .map_err(|e| lokb_core::Error::Storage(e.to_string()))?;

        let mut hits = Vec::new();
        for (score, doc_address) in top_docs {
            let doc: TantivyDocument = searcher
                .doc(doc_address)
                .map_err(|e| lokb_core::Error::Storage(e.to_string()))?;

            let privacy = doc
                .get_first(self.f_privacy_level)
                .and_then(|v| v.as_u64())
                .unwrap_or(0);

            // Filter by privacy/class
            if personal_only && privacy == 0 {
                continue;
            }
            if public_only && privacy > 0 {
                continue;
            }

            // Filter by source
            let src_name = get_text(&doc, self.f_source_name);
            if let Some(filter) = source_filter
                && src_name != filter
            {
                continue;
            }

            hits.push(FtsHit {
                chunk_id: get_text(&doc, self.f_chunk_id),
                doc_id: get_text(&doc, self.f_doc_id),
                source_id: get_text(&doc, self.f_source_id),
                source_name: src_name,
                title: get_text(&doc, self.f_title),
                text: get_text(&doc, self.f_text),
                section: {
                    let s = get_text(&doc, self.f_section);
                    if s.is_empty() { None } else { Some(s) }
                },
                score,
            });

            if hits.len() >= limit {
                break;
            }
        }

        Ok(hits)
    }

    /// Delete all chunks for a given source.
    pub fn delete_source(&self, source_id: &str) -> Result<()> {
        let mut writer: IndexWriter = self
            .index
            .writer(50_000_000)
            .map_err(|e| lokb_core::Error::Storage(e.to_string()))?;

        let term = tantivy::Term::from_field_text(self.f_source_id, source_id);
        writer.delete_term(term);
        writer
            .commit()
            .map_err(|e| lokb_core::Error::Storage(e.to_string()))?;
        self.reader
            .reload()
            .map_err(|e| lokb_core::Error::Storage(e.to_string()))?;

        Ok(())
    }
}

fn get_text(doc: &TantivyDocument, field: Field) -> String {
    doc.get_first(field)
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}
