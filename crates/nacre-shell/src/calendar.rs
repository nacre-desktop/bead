// SPDX-License-Identifier: GPL-3.0-only
//! Calendar presentation consumes a Source; month navigation stays in the view.
use bead::core::{
    Stores, Value,
    source::{self, Source},
};
use std::{collections::BTreeMap, sync::Arc};

pub struct Calendar {
    source: Arc<dyn Source>,
}
impl Calendar {
    pub fn new(source: Arc<dyn Source>) -> Self {
        Self { source }
    }
    pub fn subscribe(
        &self,
        notify: Arc<dyn Fn(bead::wayland::UiEvent) + Send + Sync>,
    ) -> source::Result<source::Subscription> {
        self.source.subscribe(Arc::new(move |_| {
            notify(bead::wayland::UiEvent::Model(BTreeMap::new()))
        }))
    }
    pub fn project(&self, stores: &Stores) -> BTreeMap<String, Value> {
        let snapshot = self.source.snapshot();
        let date = snapshot
            .data
            .get("date")
            .cloned()
            .and_then(|value| serde_json::from_value::<bead::services::LocalDate>(value).ok())
            .filter(bead::services::LocalDate::is_valid);
        let mut values = BTreeMap::from([("calendar.ready".into(), Value::Bool(date.is_some()))]);
        if let Some(date) = date {
            values.insert(
                "calendar.today".into(),
                Value::Text(format!(
                    "{:04}-{:02}-{:02}",
                    date.year, date.month, date.day
                )),
            );
            if stores.get("calendar.initialized") != Some(&Value::Bool(true)) {
                values.insert("calendar.initialized".into(), Value::Bool(true));
                values.insert("calendar.year".into(), Value::Number(date.year.into()));
                values.insert("calendar.month".into(), Value::Number(date.month.into()));
            }
        }
        values
    }
}
