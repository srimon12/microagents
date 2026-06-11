mod comparison;
mod ingest;
mod load_questions;
mod search;

use std::fs;

use clap::Parser;

use crate::{
    comparison::compare_experiment, ingest::ingest, load_questions::load_questions,
    search::search_queries,
};

/// CLI Agent built on top of the MicroAgents framework
#[derive(Parser, Debug)]
#[command(name = "search-evals")]
#[command(about, long_about = None)]
struct Args {
    /// Name of the experiment.
    #[arg(long, short)]
    experiment_name: String,

    /// Path to the questions file. Defaults to `questions.json`.
    #[arg(long, short, default_value = None)]
    questions_file: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let questions = load_questions(
        args.questions_file
            .as_ref()
            .unwrap_or(&"questions.json".to_string()),
    )?;
    ingest(&args.experiment_name).await?;
    search_queries(&args.experiment_name, questions)?;

    // Run comparison against ground-truth relevant_chunks.
    let questions = load_questions(
        args.questions_file
            .as_ref()
            .unwrap_or(&"questions.json".to_string()),
    )?;
    let metrics = compare_experiment(&args.experiment_name, &questions)?;
    let report = serde_json::to_string_pretty(&metrics)?;
    let report_path = format!("evals/experiment-{}/report.json", args.experiment_name);
    fs::write(&report_path, report)?;
    println!("Comparison report written to {}", report_path);

    Ok(())
}
