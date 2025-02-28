use crate::page::{EnumColumn, Inner, NGramStats, NGramsTableRow, NumberColumn, Page, TextColumn};
use anyhow::{bail, Result};
use num::ToPrimitive;
use pinwheel::prelude::*;
use std::sync::Arc;
use tangram_app_common::{
	error::{bad_request, not_found, redirect_to_login, service_unavailable},
	heuristics::{
		TRAINING_STATS_TEXT_COLUMN_MAX_TOKENS_TO_SHOW_IN_CHART,
		TRAINING_STATS_TEXT_COLUMN_MAX_TOKENS_TO_SHOW_IN_TABLE,
	},
	model::get_model_bytes,
	path_components,
	user::{authorize_user, authorize_user_for_model},
	Context,
};
use tangram_app_layouts::model_layout::{model_layout_info, ModelNavItem};
use tangram_id::Id;
use tangram_ui as ui;

pub async fn get(request: &mut http::Request<hyper::Body>) -> Result<http::Response<hyper::Body>> {
	let context = request.extensions().get::<Arc<Context>>().unwrap().clone();
	let mut db = match context.database_pool.begin().await {
		Ok(db) => db,
		Err(_) => return Ok(service_unavailable()),
	};
	let user = match authorize_user(request, &mut db, context.options.auth_enabled()).await? {
		Ok(user) => user,
		Err(_) => return Ok(redirect_to_login()),
	};
	let (model_id, column_name) = if let ["repos", _, "models", model_id, "training_stats", "columns", column_name] =
		path_components(request).as_slice()
	{
		(model_id.to_owned(), column_name.to_owned())
	} else {
		bail!("unexpected path");
	};
	let model_id: Id = match model_id.parse() {
		Ok(model_id) => model_id,
		Err(_) => return Ok(bad_request()),
	};
	let column_name = ui::percent_decode(column_name);
	if !authorize_user_for_model(&mut db, &user, model_id).await? {
		return Ok(not_found());
	}
	let bytes = get_model_bytes(&context.storage, model_id).await?;
	let model = tangram_model::from_bytes(&bytes)?;
	let (column_stats, target_column_stats) = match model.inner() {
		tangram_model::ModelInnerReader::Regressor(regressor) => {
			let regressor = regressor.read();
			(
				regressor.overall_column_stats(),
				regressor.overall_target_column_stats(),
			)
		}
		tangram_model::ModelInnerReader::BinaryClassifier(binary_classifier) => {
			let binary_classifier = binary_classifier.read();
			(
				binary_classifier.overall_column_stats(),
				binary_classifier.overall_target_column_stats(),
			)
		}
		tangram_model::ModelInnerReader::MulticlassClassifier(multiclass_classifier) => {
			let multiclass_classifier = multiclass_classifier.read();
			(
				multiclass_classifier.overall_column_stats(),
				multiclass_classifier.overall_target_column_stats(),
			)
		}
	};
	let column_index = column_stats
		.iter()
		.position(|column_stats| column_stats.column_name() == column_name);
	let column = if target_column_stats.column_name() == column_name {
		target_column_stats
	} else if let Some(column_index) = column_index {
		column_stats.get(column_index).unwrap()
	} else {
		return Ok(not_found());
	};

	let inner = match column {
		tangram_model::ColumnStatsReader::UnknownColumn(_) => unimplemented!(),
		tangram_model::ColumnStatsReader::NumberColumn(column_stats) => {
			let column_stats = column_stats.read();
			Inner::Number(NumberColumn {
				invalid_count: column_stats.invalid_count(),
				min: column_stats.min(),
				max: column_stats.max(),
				mean: column_stats.mean(),
				name: column_stats.column_name().to_owned(),
				p25: column_stats.p25(),
				p50: column_stats.p50(),
				p75: column_stats.p75(),
				std: column_stats.std(),
				unique_count: column_stats.unique_count(),
			})
		}
		tangram_model::ColumnStatsReader::EnumColumn(column_stats) => {
			let column_stats = column_stats.read();
			let total_count: u64 = column_stats
				.histogram()
				.iter()
				.map(|(_, count)| count)
				.sum();
			Inner::Enum(EnumColumn {
				unique_values_chart_data: Some(
					column_stats
						.histogram()
						.iter()
						.map(|(value, count)| (value.to_owned(), count))
						.collect(),
				),
				unique_values_table_rows: Some(
					column_stats
						.histogram()
						.iter()
						.map(|(value, count)| {
							(
								value.to_owned(),
								count,
								count.to_f64().unwrap() / total_count.to_f64().unwrap(),
							)
						})
						.collect(),
				),
				invalid_count: column_stats.invalid_count(),
				name: column_stats.column_name().to_owned(),
				unique_count: column_stats.unique_count(),
			})
		}
		tangram_model::ColumnStatsReader::TextColumn(column_stats) => {
			let column_stats = column_stats.read();
			let ngram_count = column_stats.top_ngrams().len();
			let mut top_ngrams_chart_values = column_stats
				.top_ngrams()
				.iter()
				.map(|(ngram, entry)| NGramStats {
					ngram: ngram.to_string(),
					row_count: entry.row_count(),
					occurrence_count: entry.occurrence_count(),
				})
				.collect::<Vec<_>>();
			top_ngrams_chart_values.sort_by(|a, b| {
				a.occurrence_count
					.partial_cmp(&b.occurrence_count)
					.unwrap()
					.reverse()
			});
			let ngrams_table_rows = top_ngrams_chart_values
				.iter()
				.take(TRAINING_STATS_TEXT_COLUMN_MAX_TOKENS_TO_SHOW_IN_TABLE)
				.cloned()
				.map(|ngram| NGramsTableRow {
					ngram: ngram.ngram,
					count: ngram.occurrence_count,
				})
				.collect();
			top_ngrams_chart_values
				.truncate(TRAINING_STATS_TEXT_COLUMN_MAX_TOKENS_TO_SHOW_IN_CHART);
			Inner::Text(TextColumn {
				name: column_stats.column_name().to_owned(),
				ngram_count,
				top_ngrams_chart_values,
				ngrams_table_rows,
			})
		}
	};
	let model_layout_info =
		model_layout_info(&mut db, &context, model_id, ModelNavItem::TrainingStats).await?;
	let page = Page {
		inner,
		model_layout_info,
	};
	let html = html(page);
	let response = http::Response::builder()
		.status(http::StatusCode::OK)
		.body(hyper::Body::from(html))
		.unwrap();
	Ok(response)
}
