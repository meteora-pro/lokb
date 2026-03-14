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
            schema,
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

    /// Create a batch writer for efficient bulk indexing.
    pub fn writer(&self) -> Result<FtsBatchWriter<'_>> {
        let writer = self
            .index
            .writer(100_000_000) // 100MB buffer for bulk
            .map_err(|e| lokb_core::Error::Storage(e.to_string()))?;
        Ok(FtsBatchWriter {
            writer,
            index: self,
            count: 0,
        })
    }

    /// Index a batch of chunks (convenience for small batches).
    pub fn index_chunks(&self, chunks: &[Chunk], source_name: &str, title: &str) -> Result<()> {
        let mut w = self.writer()?;
        w.add_chunks(chunks, source_name, title)?;
        w.commit()?;
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
            .search(&query, &TopDocs::with_limit(limit * 3))
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

            if personal_only && privacy == 0 {
                continue;
            }
            if public_only && privacy > 0 {
                continue;
            }

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

/// Batch writer for efficient bulk indexing. Commit once after all documents.
pub struct FtsBatchWriter<'a> {
    writer: IndexWriter,
    index: &'a TantivyIndex,
    count: usize,
}

impl<'a> FtsBatchWriter<'a> {
    /// Add chunks to the batch (no commit yet).
    pub fn add_chunks(&mut self, chunks: &[Chunk], source_name: &str, title: &str) -> Result<()> {
        for chunk in chunks {
            let mut doc = doc!(
                self.index.f_chunk_id => chunk.id.to_string(),
                self.index.f_doc_id => chunk.document_id.to_string(),
                self.index.f_source_id => chunk.source_id.to_string(),
                self.index.f_source_name => source_name,
                self.index.f_title => title,
                self.index.f_text => chunk.text.as_str(),
                self.index.f_content_type => format!("{:?}", chunk.content_type),
                self.index.f_privacy_level => chunk.privacy_level as u64,
            );
            if let Some(section) = &chunk.section_path {
                doc.add_text(self.index.f_section, section);
            }
            self.writer
                .add_document(doc)
                .map_err(|e| lokb_core::Error::Storage(e.to_string()))?;
            self.count += 1;
        }
        Ok(())
    }

    /// Commit all batched documents and reload reader.
    pub fn commit(mut self) -> Result<usize> {
        self.writer
            .commit()
            .map_err(|e| lokb_core::Error::Storage(e.to_string()))?;
        self.index
            .reader
            .reload()
            .map_err(|e| lokb_core::Error::Storage(e.to_string()))?;
        Ok(self.count)
    }
}

fn get_text(doc: &TantivyDocument, field: Field) -> String {
    doc.get_first(field)
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}
