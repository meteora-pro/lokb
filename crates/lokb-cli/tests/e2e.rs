use cucumber::{World, given, then, when};
use std::path::PathBuf;
use std::process::{Command, Output};
use tempfile::TempDir;

#[derive(Debug, World)]
#[world(init = Self::new)]
pub struct LokbWorld {
    /// Temporary data directory for isolation
    data_dir: TempDir,
    /// Path to the lokb binary
    binary: PathBuf,
    /// Last command output
    last_output: Option<Output>,
    /// Path to test fixtures
    fixtures_dir: PathBuf,
}

impl LokbWorld {
    fn new() -> Self {
        let binary = env!("CARGO_BIN_EXE_lokb");
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        // fixtures are at repo root: tests/fixtures/
        let fixtures_dir = PathBuf::from(manifest_dir)
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("tests")
            .join("fixtures");

        Self {
            data_dir: TempDir::new().expect("failed to create temp dir"),
            binary: PathBuf::from(binary),
            last_output: None,
            fixtures_dir,
        }
    }

    fn run_lokb(&mut self, args_str: &str) -> &Output {
        let processed = args_str
            .replace("{fixtures}", self.fixtures_dir.to_str().unwrap())
            .replace("{tmp}", self.data_dir.path().to_str().unwrap());

        let args = shell_words::split(&processed).expect("failed to parse command args");

        let output = Command::new(&self.binary)
            .args(&args)
            .env("LOKB_DATA_DIR", self.data_dir.path())
            .output()
            .expect("failed to execute lokb");

        self.last_output = Some(output);
        self.last_output.as_ref().unwrap()
    }

    fn stdout(&self) -> String {
        let output = self.last_output.as_ref().expect("no command was run");
        String::from_utf8_lossy(&output.stdout).to_string()
    }

    fn combined_output(&self) -> String {
        let output = self.last_output.as_ref().expect("no command was run");
        format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    }

    fn add_source(&mut self, name: &str, fixture_path: &str, format: &str, class: &str) {
        let full_path = self.fixtures_dir.join(fixture_path);
        let args = format!(
            "source add {} --raw {} --format {} --class {}",
            name,
            full_path.display(),
            format,
            class,
        );
        self.run_lokb(&args);
        assert!(
            self.last_output.as_ref().unwrap().status.success(),
            "failed to add source '{}': {}",
            name,
            self.combined_output()
        );
    }
}

// ── Given steps ──────────────────────────────────────────────────────

#[given("a clean lokb data directory")]
fn clean_data_dir(world: &mut LokbWorld) {
    // TempDir is already clean on creation; re-create for isolation
    world.data_dir = TempDir::new().expect("failed to create temp dir");
}

#[given(expr = "test Wikipedia articles in {string}")]
fn test_wikipedia_articles(world: &mut LokbWorld, _path: String) {
    // Just verify fixtures exist
    assert!(
        world.fixtures_dir.join("wikipedia").exists(),
        "wikipedia fixtures not found at {:?}",
        world.fixtures_dir.join("wikipedia")
    );
}

#[given(expr = "source {string} is loaded from {string} as {string} class {string}")]
fn source_is_loaded(
    world: &mut LokbWorld,
    name: String,
    path: String,
    format: String,
    class: String,
) {
    world.add_source(&name, &path, &format, &class);
}

// ── When steps ───────────────────────────────────────────────────────

#[when(expr = "I run lokb {string}")]
fn run_lokb_command(world: &mut LokbWorld, args: String) {
    world.run_lokb(&args);
}

// ── Then steps ───────────────────────────────────────────────────────

#[then("the command succeeds")]
fn command_succeeds(world: &mut LokbWorld) {
    let output = world.last_output.as_ref().expect("no command was run");
    assert!(
        output.status.success(),
        "command failed with exit code {:?}\nstdout: {}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

#[then(expr = "the output contains {string}")]
fn output_contains(world: &mut LokbWorld, expected: String) {
    let combined = world.combined_output();
    assert!(
        combined.contains(&expected),
        "expected output to contain '{}', got:\n{}",
        expected,
        combined,
    );
}

#[then(expr = "the JSON output has a source named {string}")]
fn json_has_source(world: &mut LokbWorld, name: String) {
    let stdout = world.stdout();
    let json: serde_json::Value = serde_json::from_str(&stdout).expect("invalid JSON output");
    let sources = json["sources"].as_array().expect("no 'sources' array");
    assert!(
        sources.iter().any(|s| s["name"] == name),
        "source '{}' not found in JSON output: {}",
        name,
        stdout,
    );
}

#[then("the JSON search results are not empty")]
fn json_search_not_empty(world: &mut LokbWorld) {
    let stdout = world.stdout();
    let json: serde_json::Value = serde_json::from_str(&stdout).expect("invalid JSON output");
    let results = json["results"].as_array().expect("no 'results' array");
    assert!(!results.is_empty(), "search results are empty: {}", stdout);
}

#[then(expr = "the JSON search results contain a document with title {string}")]
fn json_results_contain_title(world: &mut LokbWorld, title: String) {
    let stdout = world.stdout();
    let json: serde_json::Value = serde_json::from_str(&stdout).expect("invalid JSON output");
    let results = json["results"].as_array().expect("no 'results' array");
    assert!(
        results.iter().any(|r| r["title"] == title),
        "no result with title '{}' in: {}",
        title,
        stdout,
    );
}

#[then(expr = "the JSON search results contain a document from source {string}")]
fn json_results_contain_source(world: &mut LokbWorld, source: String) {
    let stdout = world.stdout();
    let json: serde_json::Value = serde_json::from_str(&stdout).expect("invalid JSON output");
    let results = json["results"].as_array().expect("no 'results' array");
    assert!(
        results.iter().any(|r| r["source"] == source),
        "no result from source '{}' in: {}",
        source,
        stdout,
    );
}

#[then(expr = "the JSON search results only contain documents from source {string}")]
fn json_results_only_from_source(world: &mut LokbWorld, source: String) {
    let stdout = world.stdout();
    let json: serde_json::Value = serde_json::from_str(&stdout).expect("invalid JSON output");
    let results = json["results"].as_array().expect("no 'results' array");
    for result in results {
        assert_eq!(
            result["source"], source,
            "expected all results from '{}', but found: {}",
            source, result
        );
    }
}

#[then(expr = "the export file {string} contains source {string}")]
fn export_contains_source(world: &mut LokbWorld, file_pattern: String, source: String) {
    let file_path = file_pattern.replace("{tmp}", world.data_dir.path().to_str().unwrap());
    let content = std::fs::read_to_string(&file_path)
        .unwrap_or_else(|e| panic!("failed to read export file '{}': {}", file_path, e));
    let json: serde_json::Value =
        serde_json::from_str(&content).expect("invalid JSON in export file");
    let sources = json["sources"].as_array().expect("no 'sources' in export");
    assert!(
        sources.iter().any(|s| s.as_str() == Some(&source)),
        "source '{}' not found in export: {}",
        source,
        content,
    );
}

#[then(expr = "the export file {string} does not contain source {string}")]
fn export_not_contains_source(world: &mut LokbWorld, file_pattern: String, source: String) {
    let file_path = file_pattern.replace("{tmp}", world.data_dir.path().to_str().unwrap());
    let content = std::fs::read_to_string(&file_path)
        .unwrap_or_else(|e| panic!("failed to read export file '{}': {}", file_path, e));
    let json: serde_json::Value =
        serde_json::from_str(&content).expect("invalid JSON in export file");
    let sources = json["sources"].as_array().expect("no 'sources' in export");
    assert!(
        !sources.iter().any(|s| s.as_str() == Some(&source)),
        "source '{}' should not be in export but was found: {}",
        source,
        content,
    );
}

#[then(expr = "the JSON storage {string} layer size is greater than 0")]
fn json_storage_layer_gt_zero(world: &mut LokbWorld, layer_name: String) {
    let stdout = world.stdout();
    let json: serde_json::Value = serde_json::from_str(&stdout).expect("invalid JSON output");
    let layers = json["layers"].as_array().expect("no 'layers' array");
    let layer = layers
        .iter()
        .find(|l| l["name"] == layer_name)
        .unwrap_or_else(|| panic!("layer '{}' not found in: {}", layer_name, stdout));
    let size = layer["size_bytes"]
        .as_u64()
        .expect("size_bytes not a number");
    assert!(
        size > 0,
        "expected layer '{}' size > 0, got {}",
        layer_name,
        size
    );
}

fn main() {
    let features = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("tests")
        .join("features");

    futures::executor::block_on(LokbWorld::cucumber().with_default_cli().run(features));
}
