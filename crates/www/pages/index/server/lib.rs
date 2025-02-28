use pinwheel::prelude::*;

mod inspection;
mod monitoring;
mod page;
mod predict;
mod production_metrics;
mod production_predictions;
mod production_stats;
mod train;
mod tuning;

pub fn init() -> sunfish::Page {
	sunfish::Page::new_static(|_| html(self::page::Page))
}
