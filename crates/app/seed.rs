use anyhow::Result;
use chrono::{Datelike, TimeZone, Timelike, Utc};
use clap::{ArgEnum, Parser};
use num::ToPrimitive;
use rand::Rng;
use std::{collections::HashMap, path::Path};
use tangram_app_common::monitor_event::{
	BinaryClassificationPredictOutput, MonitorEvent, MulticlassClassificationPredictOutput,
	NumberOrString, PredictOutput, PredictionMonitorEvent, RegressionPredictOutput,
	TrueValueMonitorEvent,
};
use tangram_id::Id;
use tangram_table::TableView;
use url::Url;

#[derive(Parser)]
pub struct Args {
	#[clap(long)]
	pub tangram_url: Url,
	#[clap(long, arg_enum)]
	pub dataset: Dataset,
	#[clap(long)]
	pub model_id: String,
	#[clap(long)]
	pub examples_count: usize,
}

#[derive(Clone, ArgEnum)]
pub enum Dataset {
	#[clap(name = "boston")]
	Boston,
	#[clap(name = "heart_disease")]
	HeartDisease,
	#[clap(name = "iris")]
	Iris,
}

struct DatasetConfig {
	path: &'static str,
	name: &'static str,
	target: &'static str,
	class_names: Option<&'static [&'static str]>,
}

const BOSTON: DatasetConfig = DatasetConfig {
	path: "data/boston.csv",
	name: "boston",
	target: "medv",
	class_names: None,
};

const HEART_DISEASE: DatasetConfig = DatasetConfig {
	path: "data/heart_disease.csv",
	name: "heart_disease",
	target: "diagnosis",
	class_names: Some(&["Negative", "Positive"]),
};

const IRIS: DatasetConfig = DatasetConfig {
	path: "data/iris.csv",
	name: "iris",
	target: "species",
	class_names: Some(&["Iris Setosa", "Iris Versicolor", "Iris Virginica"]),
};

pub fn main() -> Result<()> {
	let args = Args::parse();
	let dataset = match args.dataset {
		Dataset::Boston => BOSTON,
		Dataset::HeartDisease => HEART_DISEASE,
		Dataset::Iris => IRIS,
	};
	let table =
		tangram_table::Table::from_path(Path::new(dataset.path), Default::default(), &mut |_| {})?;
	let mut rng = rand::thread_rng();
	let events: Vec<MonitorEvent> = (0..args.examples_count)
		.flat_map(|_| {
			let id = Id::generate();
			let mut record = get_random_row(table.view());
			let target = record.remove(dataset.target).unwrap();
			if dataset.name == "heart_disease" {
				// Rewrite asymptomatic to asx in 50% of rows.
				if rng.gen::<bool>() {
					let chest_pain = record.get_mut("chest_pain").unwrap();
					if chest_pain == "asymptomatic" {
						*chest_pain = serde_json::Value::String("asx".to_owned());
					}
				}
			}
			let output = generate_fake_prediction(&target, &dataset);
			let model_id: &str = args.model_id.as_str();
			let date = get_random_date();
			let mut events = vec![MonitorEvent::Prediction(PredictionMonitorEvent {
				date,
				identifier: NumberOrString::String(id.to_string()),
				input: record,
				model_id: model_id.parse().unwrap(),
				options: None,
				output,
			})];
			if rng.gen::<f32>() > 0.4 {
				events.push(MonitorEvent::TrueValue(TrueValueMonitorEvent {
					model_id: model_id.parse().unwrap(),
					identifier: NumberOrString::String(id.to_string()),
					true_value: target,
					date,
				}));
			}
			events
		})
		.collect();
	for events in events.chunks(100) {
		track_events(&args.tangram_url, events);
	}
	Ok(())
}

fn generate_fake_prediction(target: &serde_json::Value, dataset: &DatasetConfig) -> PredictOutput {
	match dataset.name {
		"heart_disease" => generate_fake_prediction_heart_disease(target, dataset),
		"iris" => generate_fake_prediction_iris(target, dataset),
		"boston" => generate_fake_prediction_boston(target),
		_ => unimplemented!(),
	}
}

fn generate_fake_prediction_boston(target_value: &serde_json::Value) -> PredictOutput {
	let mut rng = rand::thread_rng();
	let target_value = target_value.as_f64().unwrap();
	let target_value = target_value + rng.gen::<f64>() * 5.0;
	PredictOutput::Regression(RegressionPredictOutput {
		value: target_value.to_f32().unwrap(),
	})
}

fn generate_fake_prediction_heart_disease(
	target_value: &serde_json::Value,
	dataset: &DatasetConfig,
) -> PredictOutput {
	let mut rng = rand::thread_rng();
	let target_value = target_value.as_str().unwrap();
	let target_value = if rng.gen::<f32>() > 0.6 {
		target_value
	} else {
		let class_names = dataset.class_names.unwrap();
		let random_target_value_index = (rng.gen::<f32>() * class_names.len().to_f32().unwrap())
			.to_usize()
			.unwrap();
		class_names[random_target_value_index]
	};
	PredictOutput::BinaryClassification(BinaryClassificationPredictOutput {
		class_name: target_value.to_string(),
		probability: 0.95,
	})
}

fn generate_fake_prediction_iris(
	target_value: &serde_json::Value,
	dataset: &DatasetConfig,
) -> PredictOutput {
	let mut rng = rand::thread_rng();
	let target_value = target_value.as_str().unwrap();
	let target_value = if rng.gen::<f32>() > 0.6 {
		target_value
	} else {
		let class_names = dataset.class_names.unwrap();
		let random_target_value_index = (rng.gen::<f32>() * class_names.len().to_f32().unwrap())
			.to_usize()
			.unwrap();
		class_names[random_target_value_index]
	};
	let probabilities = dataset
		.class_names
		.unwrap()
		.iter()
		.map(|class_name| {
			if class_name == &target_value {
				(class_name.to_string(), 0.95)
			} else {
				(class_name.to_string(), 0.025)
			}
		})
		.collect::<HashMap<String, f32>>();
	PredictOutput::MulticlassClassification(MulticlassClassificationPredictOutput {
		class_name: target_value.to_string(),
		probabilities,
	})
}

fn get_random_date() -> chrono::DateTime<Utc> {
	let end_time = chrono::Utc::now().timestamp();
	let start_time = chrono::Utc::now()
		.with_month(1)
		.unwrap()
		.with_day(1)
		.unwrap()
		.with_hour(0)
		.unwrap()
		.with_minute(0)
		.unwrap()
		.with_second(0)
		.unwrap()
		.timestamp();
	let time_range = (end_time - start_time).to_f32().unwrap();
	let timestamp = start_time
		+ (rand::thread_rng().gen::<f32>() * time_range.trunc())
			.to_i64()
			.unwrap();
	Utc.timestamp(timestamp, 0)
}

fn get_random_row(table: TableView) -> HashMap<String, serde_json::Value> {
	let mut rng = rand::thread_rng();
	let random_row_index = (table.nrows().to_f32().unwrap() * rng.gen::<f32>())
		.to_usize()
		.unwrap();
	table
		.columns()
		.iter()
		.map(|column| match column {
			tangram_table::TableColumnView::Number(column) => {
				let column_name = column.name().unwrap().to_owned();
				let value = column.data()[random_row_index].to_f64().unwrap();
				let value = if let Some(value) = serde_json::Number::from_f64(value) {
					serde_json::Value::Number(value)
				} else {
					serde_json::Value::Null
				};
				(column_name, value)
			}
			tangram_table::TableColumnView::Enum(column) => {
				let column_name = column.name().unwrap().to_owned();
				let value = column.data()[random_row_index];
				let value = match value {
					Some(value) => serde_json::Value::String(
						column.variants().get(value.get() - 1).unwrap().clone(),
					),
					None => serde_json::Value::Null,
				};
				(column_name, value)
			}
			_ => unimplemented!(),
		})
		.collect::<HashMap<String, serde_json::Value>>()
}

fn track_events(tangram_url: &Url, events: &[MonitorEvent]) {
	let client = reqwest::blocking::Client::new();
	let mut url = tangram_url.clone();
	url.set_path("/track");
	client.post(url).json(&events).send().unwrap();
}
