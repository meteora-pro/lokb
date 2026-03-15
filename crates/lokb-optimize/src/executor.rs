use lokb_core::Result;
use lokb_pipeline::{OptimizeStep, PipelineContext, StepData, StepFallback};
use std::time::Instant;

/// Configuration for a pipeline step.
pub struct StepConfig {
    pub step: Box<dyn OptimizeStep>,
    pub fallback: StepFallback,
    pub enabled: bool,
}

/// Metrics from a single step execution.
#[derive(Debug)]
pub struct StepResult {
    pub step_name: String,
    pub input_size: u64,
    pub output_size: u64,
    pub duration_ms: u64,
    pub skipped: bool,
}

/// Metrics from a full pipeline execution.
#[derive(Debug)]
pub struct PipelineResult {
    pub steps: Vec<StepResult>,
    pub total_duration_ms: u64,
}

/// Executes a chain of OptimizeSteps sequentially (ADR-002).
pub struct PipelineExecutor {
    steps: Vec<StepConfig>,
}

impl PipelineExecutor {
    pub fn new() -> Self {
        Self { steps: vec![] }
    }

    pub fn add_step(mut self, step: Box<dyn OptimizeStep>, fallback: StepFallback) -> Self {
        self.steps.push(StepConfig {
            step,
            fallback,
            enabled: true,
        });
        self
    }

    /// Execute all steps in sequence. Each step's output becomes the next step's input.
    /// Returns final transformed data and metrics.
    pub async fn execute(
        &self,
        initial_data: StepData,
        ctx: &PipelineContext,
    ) -> Result<(StepData, PipelineResult)> {
        let pipeline_start = Instant::now();
        let mut current_data = Some(initial_data);
        let mut step_results = Vec::new();

        for config in &self.steps {
            if !config.enabled {
                step_results.push(StepResult {
                    step_name: config.step.name().to_string(),
                    input_size: 0,
                    output_size: 0,
                    duration_ms: 0,
                    skipped: true,
                });
                continue;
            }

            let data = current_data
                .take()
                .expect("pipeline data consumed by previous error");
            let step_start = Instant::now();
            let input_size = estimate_data_size(&data);

            match config.step.execute(data, ctx).await {
                Ok(output) => {
                    let output_size = estimate_data_size(&output);
                    step_results.push(StepResult {
                        step_name: config.step.name().to_string(),
                        input_size,
                        output_size,
                        duration_ms: step_start.elapsed().as_millis() as u64,
                        skipped: false,
                    });
                    current_data = Some(output);
                }
                Err(e) => {
                    let skipped_result = StepResult {
                        step_name: config.step.name().to_string(),
                        input_size,
                        output_size: 0,
                        duration_ms: step_start.elapsed().as_millis() as u64,
                        skipped: true,
                    };
                    match &config.fallback {
                        StepFallback::Abort => return Err(e),
                        _ => {
                            step_results.push(skipped_result);
                            // Can't continue without data
                            break;
                        }
                    }
                }
            }
        }

        let result = PipelineResult {
            steps: step_results,
            total_duration_ms: pipeline_start.elapsed().as_millis() as u64,
        };

        match current_data {
            Some(data) => Ok((data, result)),
            None => Err(lokb_core::Error::Pipeline {
                step: "executor".to_string(),
                message: "pipeline produced no output (all steps failed)".to_string(),
            }),
        }
    }
}

impl Default for PipelineExecutor {
    fn default() -> Self {
        Self::new()
    }
}

fn estimate_data_size(data: &StepData) -> u64 {
    match data {
        StepData::Bytes(b) => b.len() as u64,
        StepData::Text { content, .. } => content.len() as u64,
        StepData::Documents(docs) => docs.iter().map(|(_, text)| text.len() as u64).sum(),
        StepData::Entities {
            entities,
            relations,
        } => (entities.len() + relations.len()) as u64,
    }
}
